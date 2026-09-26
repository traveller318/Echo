/*!
 * SOURCE OF TRUTH KEYWORDS: FakeTextPolisher, FakePolish, fake polisher, hanging polisher, polish timeout test, polish failure
 * WHAT:  FakeTextPolisher: a TextPolisher whose behaviour (FakePolish) is a pure text function, a failure, or a
 *        future that never completes; it records every input and every `prepare` and `unload` call.
 * WHY:   The polish chain's guarantees are about misbehaving stages: a slow LLM must time out and fall back to the
 *        rule output, a failing one must not block delivery (02 §8.3). `Hang` exercises the timeout path without
 *        sleeping; `Map` keeps normal-path tests deterministic.
 * WHERE: pipeline polish chain and session actor tests.
 */

use std::{future, sync::Mutex};

use super::lock;
use crate::{
    ports::TextPolisher,
    types::{
        BoxFuture, LanguageSupport, LatencyClass, PolishContext, PolisherCaps, PortError,
        PortResult,
    },
};

/// What a FakeTextPolisher does with its input.
#[derive(Debug, Clone)]
pub enum FakePolish {
    /// Returns `f(text)`.
    Map(fn(&str) -> String),
    Fail(PortError),
    /// Never completes, like an LLM that stopped answering.
    Hang,
}

#[derive(Default)]
struct PolishLog {
    inputs: Vec<String>,
    prepares: usize,
    unloads: usize,
    next_prepare_error: Option<PortError>,
}

/// A scriptable polish stage.
pub struct FakeTextPolisher {
    caps: PolisherCaps,
    behaviour: Mutex<FakePolish>,
    log: Mutex<PolishLog>,
}

impl FakeTextPolisher {
    pub fn new(caps: PolisherCaps, behaviour: FakePolish) -> Self {
        Self {
            caps,
            behaviour: Mutex::new(behaviour),
            log: Mutex::default(),
        }
    }

    /// An instant, any-language stage that needs no model.
    pub fn instant(behaviour: FakePolish) -> Self {
        Self::new(
            PolisherCaps {
                latency_class: LatencyClass::Instant,
                languages: LanguageSupport::Any,
                needs_model: false,
            },
            behaviour,
        )
    }

    /// A slow stage that needs a model, like the LLM polisher.
    pub fn slow(behaviour: FakePolish) -> Self {
        Self::new(
            PolisherCaps {
                latency_class: LatencyClass::Slow,
                languages: LanguageSupport::Any,
                needs_model: true,
            },
            behaviour,
        )
    }

    pub fn set_behaviour(&self, behaviour: FakePolish) {
        *lock(&self.behaviour) = behaviour;
    }

    pub fn fail_next_prepare(&self, error: PortError) {
        lock(&self.log).next_prepare_error = Some(error);
    }

    /// Every text `polish` received, in order.
    pub fn inputs(&self) -> Vec<String> {
        lock(&self.log).inputs.clone()
    }

    pub fn prepares(&self) -> usize {
        lock(&self.log).prepares
    }

    pub fn unloads(&self) -> usize {
        lock(&self.log).unloads
    }
}

impl TextPolisher for FakeTextPolisher {
    fn caps(&self) -> PolisherCaps {
        self.caps.clone()
    }

    fn prepare(&self) -> BoxFuture<'_, PortResult<()>> {
        Box::pin(async move {
            let mut log = lock(&self.log);
            log.prepares += 1;
            log.next_prepare_error.take().map_or(Ok(()), Err)
        })
    }

    fn polish<'a>(
        &'a self,
        text: &'a str,
        _context: &'a PolishContext,
    ) -> BoxFuture<'a, PortResult<String>> {
        lock(&self.log).inputs.push(text.to_owned());
        match lock(&self.behaviour).clone() {
            FakePolish::Map(transform) => Box::pin(future::ready(Ok(transform(text)))),
            FakePolish::Fail(error) => Box::pin(future::ready(Err(error))),
            FakePolish::Hang => Box::pin(future::pending()),
        }
    }

    fn unload(&self) {
        lock(&self.log).unloads += 1;
    }
}

#[cfg(test)]
mod tests {
    use std::task::Poll;

    use super::*;
    use crate::{
        ports::fakes::poll_once,
        types::{AppError, StaticList},
    };

    fn context() -> PolishContext {
        PolishContext {
            language: None,
            punctuated: true,
            cased: true,
            remove_fillers: true,
            dictionary: StaticList::from(Vec::new()),
        }
    }

    #[test]
    fn maps_fails_or_hangs() {
        let polisher = FakeTextPolisher::instant(FakePolish::Map(str::to_uppercase));
        let context = context();
        assert!(matches!(
            poll_once(polisher.polish("hi", &context)),
            Poll::Ready(Ok(text)) if text == "HI"
        ));
        polisher.set_behaviour(FakePolish::Fail(AppError::Polish.into()));
        assert!(matches!(
            poll_once(polisher.polish("hi", &context)),
            Poll::Ready(Err(_))
        ));
        polisher.set_behaviour(FakePolish::Hang);
        assert!(poll_once(polisher.polish("hi", &context)).is_pending());
        assert_eq!(polisher.inputs(), ["hi", "hi", "hi"]);
    }

    #[test]
    fn prepare_is_counted_and_can_fail_once() {
        let polisher = FakeTextPolisher::slow(FakePolish::Hang);
        assert_eq!(polisher.caps().latency_class, LatencyClass::Slow);
        polisher.fail_next_prepare(PortError::new(AppError::ModelMissing {
            model_id: crate::types::ModelId::from_static("llm"),
        }));
        assert!(matches!(poll_once(polisher.prepare()), Poll::Ready(Err(_))));
        assert!(matches!(poll_once(polisher.prepare()), Poll::Ready(Ok(()))));
        assert_eq!(polisher.prepares(), 2);
    }
}
