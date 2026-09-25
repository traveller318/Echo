/*!
 * SOURCE OF TRUTH KEYWORDS: PolishChains, shared polish chain, PolisherBuilder, chain per settings, rebuild on plan change, one LLM sidecar, warm rebuilt chain
 * WHAT:  PolishChains: the one polish chain of the app, shared by every path that polishes text. `for_settings`
 *        returns the chain for the settings in effect, rebuilding it (reusing unchanged stages) only when
 *        `polish_plan` changed, and warming a rebuilt chain in the background.
 * WHY:   A live take and a History retry must polish through the same stages (02 §8.3), and a stage can own an
 *        expensive resource (the LLM sidecar, 05 A13): two chains would start two sidecars. Cloning shares the one
 *        chain, and the lock only guards the swap, never a polish run (the chain is handed out as an Arc). A
 *        rebuilt stage (the LLM just switched on) starts warming at once; a take that runs before it is ready falls
 *        back (chain.rs). The first build is warmed by the session's prepare on RunEvent::Ready. Stages are built by
 *        a PolisherBuilder: the registry in the app, fakes in tests.
 * WHERE: Held in SessionEngines (pipeline/session/actor.rs), shared by the session runner (deliver, prepare) and
 *        CommandCtx (session_retry through pipeline/retry.rs).
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

    /// The chain for `settings`; must be called inside a tokio runtime (a rebuilt chain warms on a task).
    pub fn for_settings(&self, settings: &SettingsSnapshot) -> Arc<PolishChain> {
        let plan = polish_plan(settings);
        let mut current = self.inner.current.lock();
        if let Some(chain) = current.as_ref()
            && chain.plan() == &plan
        {
            return Arc::clone(chain);
        }
        let rebuilt = current.is_some();
        let build = &self.inner.build;
        let chain = Arc::new(PolishChain::build_with(plan, current.as_deref(), |id| {
            build(id)
        }));
        *current = Some(Arc::clone(&chain));
        drop(current);
        if rebuilt {
            // A new stage (the LLM sidecar) starts warming now; a take that runs first falls back.
            let warming = Arc::clone(&chain);
            tokio::spawn(async move {
                warming.prepare().await;
            });
        }
        chain
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
