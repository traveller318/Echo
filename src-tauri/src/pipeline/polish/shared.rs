/*!
 * SOURCE OF TRUTH KEYWORDS: PolishChains, shared polish chain, PolisherBuilder, chain per settings, rebuild on plan change, one LLM sidecar, warm rebuilt chain, refresh_on_change, unload stage, prepare_now
 * WHAT:  PolishChains: the one polish chain of the app, shared by every path that polishes text. `for_settings`
 *        returns the chain for the settings in effect, rebuilding it (reusing unchanged stages) only when
 *        `polish_plan` changed, warming a rebuilt chain in the background and unloading the stages it dropped;
 *        `refresh_on_change` does that at once after a settings write; `prepare_now` warms the current chain again
 *        (a model was just installed); `unload_stage` makes one stage let go of its files.
 * WHY:   A live take and a History retry must polish through the same stages (02 §8.3), and a stage can own an
 *        expensive resource (the LLM sidecar, 05 A13): two chains would start two sidecars. Cloning shares the one
 *        chain, and the lock only guards the swap, never a polish run (the chain is handed out as an Arc). A
 *        rebuilt stage (the LLM just switched on) starts warming at once; a take that runs before it is ready falls
 *        back (chain.rs). The first build is warmed by the session's prepare on RunEvent::Ready. Stages are built by
 *        a PolisherBuilder: the registry in the app, fakes in tests. Switching grammar polish off must stop the
 *        sidecar now, not when the last take lets go of the old chain, so a dropped stage is unloaded as soon as the
 *        chain is rebuilt (05 A13: started when enabled, stopped when disabled). Warming needs the async runtime; a
 *        caller without one (a unit test) gets the chain unwarmed, and the next take warms it.
 * WHERE: Held in SessionEngines (pipeline/session/actor.rs), shared by the session runner (deliver, prepare),
 *        CommandCtx (session_retry through pipeline/retry.rs), SettingsEffects (`refresh_on_change`) and the model
 *        manager (`unload_stage`, `prepare_now` around installs and removals).
 */

use std::sync::Arc;

use parking_lot::Mutex;

use super::{PolishChain, polish_plan};
use crate::{
    ports::TextPolisher,
    types::{EngineId, PortResult, SettingsSnapshot},
};

/// Builds a polish stage by its registry id.
pub type PolisherBuilder =
    Arc<dyn Fn(&EngineId) -> PortResult<Arc<dyn TextPolisher>> + Send + Sync>;

/// The app's polish chain, rebuilt when the settings change what it runs; clones share it.
#[derive(Clone)]
pub struct PolishChains {
    inner: Arc<Inner>,
}

struct Inner {
    build: PolisherBuilder,
    current: Mutex<Option<Arc<PolishChain>>>,
}

impl PolishChains {
    /// No chain yet; the first `for_settings` builds it with `build`.
    pub fn new(build: PolisherBuilder) -> Self {
        Self {
            inner: Arc::new(Inner {
                build,
                current: Mutex::new(None),
            }),
        }
    }

    /// The chain for `settings`; a rebuilt chain warms on the async runtime when there is one.
    pub fn for_settings(&self, settings: &SettingsSnapshot) -> Arc<PolishChain> {
        let plan = polish_plan(settings);
        let mut current = self.inner.current.lock();
        if let Some(chain) = current.as_ref()
            && chain.plan() == &plan
        {
            return Arc::clone(chain);
        }
        let build = &self.inner.build;
        let chain = Arc::new(PolishChain::build_with(plan, current.as_deref(), |id| {
            build(id)
        }));
        let previous = current.replace(Arc::clone(&chain));
        drop(current);
        if let Some(previous) = previous {
            previous.unload_stages_missing_from(&chain);
            // A new stage (the LLM sidecar) starts warming now; a take that runs first falls back.
            warm(Arc::clone(&chain));
        }
        chain
    }

    /// Rebuilds the chain now when a settings write changed what it runs (starting or stopping the LLM stage).
    pub fn refresh_on_change(&self, before: &SettingsSnapshot, after: &SettingsSnapshot) {
        if polish_plan(before) != polish_plan(after) {
            self.for_settings(after);
        }
    }

    /// Warms the chain for `settings` in the background (a stage whose model just arrived starts now).
    pub fn prepare_now(&self, settings: &SettingsSnapshot) {
        warm(self.for_settings(settings));
    }

    /// Asks stage `id` of the current chain to release its files; false when the chain does not run it.
    pub fn unload_stage(&self, id: &EngineId) -> bool {
        self.inner
            .current
            .lock()
            .as_ref()
            .is_some_and(|chain| chain.unload_stage(id))
    }
}

/// Prepares `chain` on the async runtime; without one it stays unwarmed until its first take.
fn warm(chain: Arc<PolishChain>) {
    match tokio::runtime::Handle::try_current() {
        Ok(runtime) => {
            runtime.spawn(async move {
                chain.prepare().await;
            });
        }
        Err(_) => tracing::debug!("no async runtime; the polish chain warms on its first take"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        registry::{
            self,
            engines::BuildCtx,
            settings::{defaults, keys, resolve},
        },
        types::{AppPaths, SettingValue},
    };

    fn chains() -> PolishChains {
        let root = std::env::temp_dir().join("echo-polish-chains");
        let ctx = BuildCtx {
            paths: AppPaths::new(root.join("data"), root.join("resources")),
        };
        PolishChains::new(Arc::new(move |id: &EngineId| {
            registry::engines::build_polisher(id, &ctx)
        }))
    }

    #[test]
    fn clones_share_one_chain_until_the_plan_changes() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        // A rebuild spawns its warm-up, which needs a runtime.
        let _entered = runtime.enter();
        let chains = chains();
        let settings = defaults();
        let first = chains.for_settings(&settings);
        let shared = chains.clone().for_settings(&settings);
        assert!(Arc::ptr_eq(&first, &shared));

        let changed = resolve([(keys::TRAILING_SPACE, SettingValue::Bool(false))]);
        let rebuilt = chains.for_settings(&changed);
        assert!(!Arc::ptr_eq(&first, &rebuilt));
        assert!(!rebuilt.plan().trailing_space);
        assert!(Arc::ptr_eq(&rebuilt, &chains.for_settings(&changed)));
    }
}
