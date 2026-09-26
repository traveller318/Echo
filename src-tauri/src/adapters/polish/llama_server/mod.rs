/*!
 * SOURCE OF TRUTH KEYWORDS: LlamaServerPolisher, LLM polisher, llama.cpp sidecar polisher, grammar polish adapter, Qwen3 polish, TextPolisher LLM, opt-in LLM stage
 * WHAT:  LlamaServerPolisher: the TextPolisher that rewrites a take's text for grammar through a local llama-server
 *        sidecar (sidecar.rs). `prepare` starts the sidecar and waits until it is ready; `polish` sends one chat
 *        request (request.rs) and returns the answer only when the safety filter accepts it (safety.rs); `unload`
 *        and drop stop the sidecar.
 * WHY:   02 §8.3 stage 6: opt-in, local, never blocking delivery. `polish` never waits for a sidecar that is not
 *        ready: it asks it to start and fails at once, so the chain keeps the rule output for that take and the next
 *        take gets the LLM (a killed sidecar comes back by itself, 05 A13). The chain's 2 s timeout bounds a slow
 *        answer; dropping the request future closes its connection and llama-server cancels the generation. A text
 *        longer than the context allows is refused before it is sent. Everything model-specific (prompt, thinking
 *        switch, budget, safety limits, context, GPU use, file locations) comes from the registry as a
 *        LlamaServerSetup, so another GGUF model is a manifest and a profile, not new code. Errors are `Polish`
 *        (the chain logs the detail, never the text) or `ModelMissing` while the files are not installed.
 * WHERE: Built by the registry's model polisher entry (registry/engines.rs, `polish.llm_engine`); run by
 *        pipeline/polish (PolishChain) as its slow stage; unloaded by the model manager before its files change.
 */

mod request;
mod safety;
mod sidecar;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use sidecar::{Sidecar, SidecarPhase};

use crate::{
    adapters::net::LoopbackClient,
    ports::TextPolisher,
    types::{
        AppError, BoxFuture, LanguageSupport, LatencyClass, LlamaServerSetup, PolishContext,
        PolisherCaps, PortError, PortResult,
    },
};

/// Grammar polish through a local llama-server.
pub struct LlamaServerPolisher {
    sidecar: Arc<Sidecar>,
}

impl LlamaServerPolisher {
    /// Slow (hundreds of milliseconds, under the chain's timeout), any language the model writes, needs a model.
    pub const CAPS: PolisherCaps = PolisherCaps {
        latency_class: LatencyClass::Slow,
        languages: LanguageSupport::Any,
        needs_model: true,
    };

    /// A polisher over `setup`; nothing starts until `prepare` or the first `polish`.
    pub fn new(setup: LlamaServerSetup) -> PortResult<Self> {
        Ok(Self {
            sidecar: Arc::new(Sidecar::new(setup, LoopbackClient::new(AppError::Polish)?)),
        })
    }

    /// A polisher talking to a server already listening on `port` with `key` (tests use a fake server).
    #[cfg(test)]
    fn attached(setup: LlamaServerSetup, port: u16, key: &str) -> PortResult<Self> {
        Ok(Self {
            sidecar: Arc::new(Sidecar::attached(
                setup,
                LoopbackClient::new(AppError::Polish)?,
                port,
                key,
            )),
        })
    }

    /// Sends `text` and returns the accepted answer.
    async fn ask(&self, port: u16, text: &str) -> PortResult<String> {
        let setup = self.sidecar.setup();
        if !request::fits_context(&setup.profile, text, setup.policy.context_tokens) {
            return Err(PortError::new(AppError::Polish)
                .with_detail("the text is too long for the grammar model's context"));
        }
        let response = self
            .sidecar
            .http()
            .post_json(
                port,
                request::CHAT_PATH,
                self.sidecar.key(),
                request::chat_body(&setup.profile, text),
            )
            .await?;
        if response.status != 200 {
            return Err(PortError::new(AppError::Polish).with_detail(format!(
                "the grammar model answered HTTP {}",
                response.status
            )));
        }
        let (answer, finish) = request::parse_answer(&response.body)?;
        safety::accept(&setup.profile, text, &answer, finish).map_err(|rejection| {
            PortError::new(AppError::Polish).with_detail(format!(
                "the grammar model's answer was not used: {rejection:?}"
            ))
        })
    }
}

impl TextPolisher for LlamaServerPolisher {
    fn caps(&self) -> PolisherCaps {
        Self::CAPS
    }

    fn prepare(&self) -> BoxFuture<'_, PortResult<()>> {
        Box::pin(async move {
            self.sidecar.start(true);
            match self.sidecar.settle().await {
                SidecarPhase::Ready { .. } => Ok(()),
                SidecarPhase::Missing(error) | SidecarPhase::GaveUp(error) => Err(error.into()),
                phase => Err(PortError::new(AppError::Polish)
                    .with_detail(format!("the grammar model is not running ({phase:?})"))),
            }
        })
    }

    fn polish<'a>(
        &'a self,
        text: &'a str,
        _context: &'a PolishContext,
    ) -> BoxFuture<'a, PortResult<String>> {
        Box::pin(async move {
            let Some(port) = self.sidecar.ready_port() else {
                // Not installed: say so, and start nothing (a start would only find the same files missing).
                if let Some(missing) = self.sidecar.missing() {
                    return Err(missing.into());
                }
                let phase = self.sidecar.phase();
                self.sidecar.start(false);
                return Err(PortError::new(AppError::Polish)
                    .with_detail(format!("the grammar model is not ready ({phase:?})")));
            };
            self.ask(port, text).await
        })
    }

    fn unload(&self) {
        self.sidecar.stop();
    }
}

impl Drop for LlamaServerPolisher {
    fn drop(&mut self) {
        self.sidecar.stop();
    }
}
