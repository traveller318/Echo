/*!
 * SOURCE OF TRUTH KEYWORDS: PolishChain, polish chain, run polish stages, slow stage timeout, fallback to rule output, trailing space, stage reuse
 * WHAT:  PolishChain: the built stages of a PolishPlan. `run` passes a take's text through each stage in order and
 *        appends the trailing space last, returning a PolishOutcome; `prepare` readies every stage (e.g. starts
 *        the LLM sidecar); `build` makes the stages through the registry, reusing the stages of a previous chain.
 * WHY:   02 §8.3: the order is fixed, and a stage that cannot help never blocks delivery. A slow stage (an LLM) runs
 *        under the PolishPolicy timeout; a timeout, an error or empty output keeps the text of the stage before it
 *        and is reported as a fallback. Instant stages are awaited directly: they are synchronous work behind a
 *        ready future, which a timer could not interrupt anyway. A stage whose caps exclude the take's language is
 *        skipped, not failed. A stage that could not be built is reported on every take, so the session can say
 *        why grammar polish did nothing, and is retried by the next build. Rebuilding after a settings change keeps
 *        the Arc of every stage whose id is unchanged, so a running sidecar is not restarted. Fallback details are
 *        logged here, never the text (02 §12).
 * WHERE: The session actor owns one chain, rebuilt from `polish_plan` after SettingsChanged, and runs it on the
 *        joined segment text of each take; tests build it from port fakes with `build_with`.
 */

use std::sync::Arc;

use crate::{
    ports::TextPolisher,
    registry::{self, engines::BuildCtx},
    types::{
        EngineId, LatencyClass, PolishContext, PolishFallback, PolishFallbackReason, PolishOutcome,
        PolishPlan, PolishPolicy, PolisherCaps, PortResult,
    },
};

/// A built stage.
struct Stage {
    id: EngineId,
    caps: PolisherCaps,
    polisher: Arc<dyn TextPolisher>,
}

/// The polish stages of a plan, ready to run.
pub struct PolishChain {
    plan: PolishPlan,
    policy: PolishPolicy,
    stages: Vec<Stage>,
    /// Planned stages that could not be built, reported on every run.
    unavailable: Vec<PolishFallback>,
}

impl PolishChain {
    /// Builds `plan` through the registry, reusing the stages `previous` already built.
    pub fn build(plan: PolishPlan, ctx: &BuildCtx, previous: Option<&Self>) -> Self {
        Self::build_with(plan, previous, |id| {
            registry::engines::build_polisher(id, ctx)
        })
    }

    /// Builds `plan` with `build`, reusing the stages `previous` already built.
    pub fn build_with(
        plan: PolishPlan,
        previous: Option<&Self>,
        build: impl Fn(&EngineId) -> PortResult<Arc<dyn TextPolisher>>,
    ) -> Self {
        let mut stages = Vec::with_capacity(plan.stages.len());
        let mut unavailable = Vec::new();
        for id in &plan.stages {
            let reused = previous
                .and_then(|chain| chain.stages.iter().find(|stage| stage.id == *id))
                .map(|stage| Arc::clone(&stage.polisher));
            match reused.map_or_else(|| build(id), Ok) {
                Ok(polisher) => stages.push(Stage {
                    id: id.clone(),
                    caps: polisher.caps(),
                    polisher,
                }),
                Err(error) => {
                    tracing::warn!(
                        engine = %id,
                        code = %error.error(),
                        detail = error.detail(),
                        "polish stage unavailable; its step is skipped"
                    );
                    unavailable.push(PolishFallback {
                        engine_id: id.clone(),
                        reason: PolishFallbackReason::Unavailable(error.into_app_error()),
                    });
                }
            }
        }
        Self {
            plan,
            policy: previous.map_or(PolishPolicy::DEFAULT, |chain| chain.policy),
            stages,
            unavailable,
        }
    }

    /// The same chain with `policy` (tests shorten the slow-stage timeout).
    #[must_use]
    pub const fn with_policy(mut self, policy: PolishPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// The plan this chain was built from; compare it with a fresh `polish_plan` to know when to rebuild.
    pub const fn plan(&self) -> &PolishPlan {
        &self.plan
    }

    /// Ids of the stages that were built, in order.
    pub fn stage_ids(&self) -> impl Iterator<Item = &EngineId> {
        self.stages.iter().map(|stage| &stage.id)
    }

    /// Asks stage `id` to release what it holds (a sidecar, open model files); false when the chain has no such
    /// stage.
    pub fn unload_stage(&self, id: &EngineId) -> bool {
        self.stages
            .iter()
            .find(|stage| stage.id == *id)
            .map(|stage| stage.polisher.unload())
            .is_some()
    }

    /// Unloads every stage of this chain that `next` does not run (a stage the settings turned off).
    pub fn unload_stages_missing_from(&self, next: &Self) {
        for stage in &self.stages {
            if !next.stages.iter().any(|kept| kept.id == stage.id) {
                stage.polisher.unload();
            }
        }
    }

    /// Readies every stage; the stages that failed, with why (they stay in the chain and fall back per take).
    pub async fn prepare(&self) -> Vec<PolishFallback> {
        let mut failed = Vec::new();
        for stage in &self.stages {
            if let Err(error) = stage.polisher.prepare().await {
                tracing::warn!(
                    engine = %stage.id,
                    code = %error.error(),
                    detail = error.detail(),
                    "polish stage could not get ready"
                );
                failed.push(PolishFallback {
                    engine_id: stage.id.clone(),
                    reason: PolishFallbackReason::Failed(error.into_app_error()),
                });
            }
        }
        failed
    }

    /// Polishes `text`: every stage in order, then the trailing space.
    pub async fn run(&self, text: &str, context: &PolishContext) -> PolishOutcome {
        let mut outcome = PolishOutcome {
            text: text.trim().to_owned(),
            polisher_ids: Vec::new(),
            fallbacks: self.unavailable.clone(),
        };
        for stage in &self.stages {
            if outcome.text.is_empty() {
                break;
            }
            if context
                .language
                .as_ref()
                .is_some_and(|language| !stage.caps.languages.supports(language))
            {
                continue;
            }
            match self.run_stage(stage, &outcome.text, context).await {
                Ok(polished) => {
                    outcome.text = polished;
                    outcome.polisher_ids.push(stage.id.clone());
                }
                Err(reason) => outcome.fallbacks.push(PolishFallback {
                    engine_id: stage.id.clone(),
                    reason,
                }),
            }
        }
        // A stage may leave edge whitespace (an LLM's final newline); the plan alone decides how the text ends.
        let trimmed = outcome.text.trim();
        if trimmed.len() != outcome.text.len() {
            outcome.text = trimmed.to_owned();
        }
        if self.plan.trailing_space && !outcome.text.is_empty() {
            outcome.text.push(' ');
        }
        outcome
    }

    /// One stage's text, or why it is not used.
    async fn run_stage(
        &self,
        stage: &Stage,
        text: &str,
        context: &PolishContext,
    ) -> Result<String, PolishFallbackReason> {
        let slow = stage.caps.latency_class == LatencyClass::Slow;
        let result = if slow {
            let timeout = self.policy.slow_stage_timeout();
            tokio::time::timeout(timeout, stage.polisher.polish(text, context))
                .await
                .map_err(|_| {
                    tracing::warn!(
                        engine = %stage.id,
                        timeout_ms = self.policy.slow_stage_timeout_ms,
                        "polish stage timed out; kept the previous text"
                    );
                    PolishFallbackReason::TimedOut
                })?
        } else {
            stage.polisher.polish(text, context).await
        };
        match result {
            Ok(polished) if slow && polished.trim().is_empty() => {
                tracing::warn!(engine = %stage.id, "polish stage returned no text; kept the previous text");
                Err(PolishFallbackReason::EmptyOutput)
            }
            Ok(polished) => Ok(polished),
            Err(error) => {
                tracing::warn!(
                    engine = %stage.id,
                    code = %error.error(),
                    detail = error.detail(),
                    "polish stage failed; kept the previous text"
                );
                Err(PolishFallbackReason::Failed(error.into_app_error()))
            }
        }
    }
}
