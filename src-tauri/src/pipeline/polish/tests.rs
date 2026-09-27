/*!
 * SOURCE OF TRUTH KEYWORDS: polish chain tests, PolishChain fakes, slow stage timeout test, fallback test, polish budget test, 2-minute transcript
 * WHAT:  Tests of the polish pipeline: the plan from settings, the context from caps, the chain's order, fallbacks,
 *        language skip, trailing space and stage reuse (with the port fakes), and the real registry chain end to end,
 *        including its time on a 2-minute transcript against the 5 ms budget (02 §6.2).
 * WHY:   The chain's promises are about misbehaving stages (02 §8.3); fakes script them deterministically, and a
 *        shortened PolishPolicy keeps the timeout test fast.
 * WHERE: `cargo test` (pipeline::polish::tests).
 */

use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use super::*;
use crate::{
    ports::{
        TextPolisher,
        fakes::{FakePolish, FakeTextPolisher},
    },
    registry::{
        self,
        engines::{BuildCtx, EngineEntry, PARAKEET_TDT_V3, RULE_POLISHER},
        settings::{defaults, keys, resolve},
    },
    types::{
        AppError, AppPaths, AsrCaps, EngineId, Language, LanguageSupport, LatencyClass,
        PolishContext, PolishFallback, PolishFallbackReason, PolishPlan, PolishPolicy,
        PolisherCaps, PortError, PortResult, SettingValue, StaticList, StaticStr, TextPair,
    },
};

const RULES: EngineId = EngineId::from_static("fake-rules");
const LLM: EngineId = EngineId::from_static("fake-llm");
const QWEN: &str = "qwen3-1.7b";
const ENGLISH: &[Language] = &[Language::from_static("en")];

fn block_on<T>(future: impl Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap()
        .block_on(future)
}

/// The caps of the default speech engine, read from its registry entry.
fn parakeet_caps() -> AsrCaps {
    registry::engines::find(&PARAKEET_TDT_V3)
        .and_then(EngineEntry::asr_caps)
        .cloned()
        .unwrap()
}

fn plan(stages: &[EngineId], trailing_space: bool) -> PolishPlan {
    PolishPlan {
        stages: stages.to_vec(),
        trailing_space,
    }
}

fn context(language: Option<&'static str>) -> PolishContext {
    PolishContext {
        language: language.map(Language::from_static),
        punctuated: true,
        cased: true,
        remove_fillers: true,
        dictionary: StaticList::new(&[]),
    }
}

fn exclaim(text: &str) -> String {
    format!("{text}!")
}

/// A chain of an instant rules fake (uppercase) and a slow LLM fake doing `llm`.
fn fake_chain(llm: FakePolish, trailing_space: bool) -> (PolishChain, Arc<FakeTextPolisher>) {
    let rules = Arc::new(FakeTextPolisher::instant(FakePolish::Map(
        str::to_uppercase,
    )));
    let slow = Arc::new(FakeTextPolisher::slow(llm));
    let (rules_stage, slow_stage) = (Arc::clone(&rules), Arc::clone(&slow));
    let chain = PolishChain::build_with(plan(&[RULES, LLM], trailing_space), None, move |id| {
        let stage: Arc<dyn TextPolisher> = if *id == RULES {
            Arc::clone(&rules_stage) as Arc<dyn TextPolisher>
        } else {
            Arc::clone(&slow_stage) as Arc<dyn TextPolisher>
        };
        Ok(stage)
    })
    .with_policy(PolishPolicy {
        slow_stage_timeout_ms: 20,
    });
    (chain, slow)
}

fn reasons(fallbacks: &[PolishFallback]) -> Vec<(&str, &PolishFallbackReason)> {
    fallbacks
        .iter()
        .map(|fallback| (fallback.engine_id.as_str(), &fallback.reason))
        .collect()
}

#[test]
fn the_plan_is_the_rules_then_the_selected_model_stage() {
    let default_plan = polish_plan(&defaults());
    assert_eq!(default_plan.stages, [RULE_POLISHER]);
    assert!(default_plan.trailing_space);

    let enabled = resolve([(keys::LLM_ENABLED, SettingValue::Bool(true))]);
    assert_eq!(
        polish_plan(&enabled).stages,
        [RULE_POLISHER, EngineId::from_static(QWEN)],
        "the LLM keeps its slot even before its engine is registered"
    );

    let no_space = resolve([(keys::TRAILING_SPACE, SettingValue::Bool(false))]);
    assert!(!polish_plan(&no_space).trailing_space);
}

#[test]
fn the_context_follows_the_settings_and_the_engine_caps() {
    let pair = TextPair {
        from: StaticStr::new("cloud code"),
        to: StaticStr::new("Claude Code"),
    };
    let settings = resolve([
        (keys::REMOVE_FILLERS, SettingValue::Bool(false)),
        (
            keys::DICTIONARY,
            SettingValue::Pairs(StaticList::from(vec![pair.clone()])),
        ),
    ]);
    let caps = AsrCaps {
        languages: StaticList::new(&[]),
        auto_language: true,
        punctuation: true,
        casing: false,
        accelerators: StaticList::new(&[]),
        max_segment_s: 20,
    };
    let context = polish_context(&settings, &caps, Some(Language::from_static("en")));
    assert_eq!(context.language, Some(Language::from_static("en")));
    assert!(context.punctuated);
    assert!(!context.cased);
    assert!(!context.remove_fillers);
    assert_eq!(&*context.dictionary, [pair]);
}

#[test]
fn a_switched_off_dictionary_reaches_no_stage() {
    let pair = TextPair {
        from: StaticStr::new("cloud code"),
        to: StaticStr::new("Claude Code"),
    };
    let settings = resolve([
        (keys::DICTIONARY_ENABLED, SettingValue::Bool(false)),
        (
            keys::DICTIONARY,
            SettingValue::Pairs(StaticList::from(vec![pair])),
        ),
    ]);
    let context = polish_context(&settings, &parakeet_caps(), None);
    assert!(
        context.dictionary.is_empty(),
        "the saved terms stay, unused"
    );

    let chain = registry_chain(&settings);
    let outcome = block_on(chain.run("I use cloud code.", &context));
    assert_eq!(outcome.text, "I use cloud code. ");
}

#[test]
fn stages_run_in_order_and_the_trailing_space_comes_last() {
    let (chain, slow) = fake_chain(FakePolish::Map(exclaim), true);
    assert_eq!(chain.stage_ids().collect::<Vec<_>>(), [&RULES, &LLM]);
    let outcome = block_on(chain.run("  hello  ", &context(None)));
    assert_eq!(outcome.text, "HELLO! ");
    assert_eq!(outcome.polisher_ids, [RULES, LLM]);
    assert!(outcome.fallbacks.is_empty());
    assert_eq!(slow.inputs(), ["HELLO"], "the LLM sees the rule output");
}

#[test]
fn a_slow_stage_that_hangs_times_out_to_the_rule_output() {
    let (chain, _) = fake_chain(FakePolish::Hang, false);
    let started = Instant::now();
    let outcome = block_on(chain.run("hello", &context(None)));
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(outcome.text, "HELLO");
    assert_eq!(outcome.polisher_ids, [RULES]);
    assert_eq!(
        reasons(&outcome.fallbacks),
        [("fake-llm", &PolishFallbackReason::TimedOut)]
    );
}

#[test]
fn a_failing_or_empty_slow_stage_falls_back() {
    let (failing, _) = fake_chain(FakePolish::Fail(AppError::Polish.into()), false);
    let outcome = block_on(failing.run("hello", &context(None)));
    assert_eq!(outcome.text, "HELLO");
    assert_eq!(
        reasons(&outcome.fallbacks),
        [("fake-llm", &PolishFallbackReason::Failed(AppError::Polish))]
    );

    let (empty, _) = fake_chain(FakePolish::Map(|_| "  \n".to_owned()), false);
    let outcome = block_on(empty.run("hello", &context(None)));
    assert_eq!(outcome.text, "HELLO");
    assert_eq!(
        reasons(&outcome.fallbacks),
        [("fake-llm", &PolishFallbackReason::EmptyOutput)]
    );
}

#[test]
fn a_model_stage_that_rewrites_a_dictionary_term_falls_back_to_the_rule_output() {
    let with_term = PolishContext {
        dictionary: StaticList::from(vec![TextPair {
            from: StaticStr::new("bridge mind"),
            to: StaticStr::new("BRIDGEMIND"),
        }]),
        ..context(None)
    };
    // The rules fake uppercases, so its output holds the spelling "BRIDGEMIND".
    let (rewriting, _) = fake_chain(FakePolish::Map(str::to_lowercase), false);
    let outcome = block_on(rewriting.run("i use bridgemind", &with_term));
    assert_eq!(outcome.text, "I USE BRIDGEMIND");
    assert_eq!(outcome.polisher_ids, [RULES]);
    assert_eq!(
        reasons(&outcome.fallbacks),
        [("fake-llm", &PolishFallbackReason::DictionaryTermLost)]
    );

    let (keeping, _) = fake_chain(FakePolish::Map(exclaim), false);
    let outcome = block_on(keeping.run("i use bridgemind", &with_term));
    assert_eq!(outcome.text, "I USE BRIDGEMIND!");
    assert_eq!(outcome.polisher_ids, [RULES, LLM]);
    assert!(outcome.fallbacks.is_empty());
}

#[test]
fn an_instant_stage_may_empty_the_text_and_nothing_runs_after_it() {
    let rules = Arc::new(FakeTextPolisher::instant(FakePolish::Map(|_| {
        String::new()
    })));
    let slow = Arc::new(FakeTextPolisher::slow(FakePolish::Map(exclaim)));
    let (rules_stage, slow_stage) = (Arc::clone(&rules), Arc::clone(&slow));
    let chain = PolishChain::build_with(plan(&[RULES, LLM], true), None, move |id| {
        Ok(if *id == RULES {
            Arc::clone(&rules_stage) as Arc<dyn TextPolisher>
        } else {
            Arc::clone(&slow_stage) as Arc<dyn TextPolisher>
        })
    });
    let outcome = block_on(chain.run("um", &context(None)));
    assert_eq!(outcome.text, "", "no trailing space on an empty take");
    assert_eq!(outcome.polisher_ids, [RULES]);
    assert!(slow.inputs().is_empty());

    let blank = block_on(chain.run("   ", &context(None)));
    assert_eq!(blank.text, "");
    assert!(blank.polisher_ids.is_empty());
}

#[test]
fn a_stage_for_other_languages_is_skipped() {
    let english_only = PolisherCaps {
        latency_class: LatencyClass::Instant,
        languages: LanguageSupport::Only {
            languages: StaticList::new(ENGLISH),
        },
        needs_model: false,
    };
    let chain = PolishChain::build_with(plan(&[RULES], false), None, move |_| {
        Ok(Arc::new(FakeTextPolisher::new(
            english_only.clone(),
            FakePolish::Map(str::to_uppercase),
        )) as Arc<dyn TextPolisher>)
    });
    let german = block_on(chain.run("hallo", &context(Some("de"))));
    assert_eq!(german.text, "hallo");
    assert!(german.polisher_ids.is_empty());
    assert!(german.fallbacks.is_empty());
    assert_eq!(block_on(chain.run("hi", &context(Some("en")))).text, "HI");
    assert_eq!(block_on(chain.run("hi", &context(None))).text, "HI");
}

#[test]
fn unbuildable_stages_are_reported_on_every_run_and_retried_on_rebuild() {
    let builds = AtomicUsize::new(0);
    let build = |id: &EngineId| -> PortResult<Arc<dyn TextPolisher>> {
        builds.fetch_add(1, Ordering::SeqCst);
        if *id == LLM {
            Err(PortError::new(AppError::ModelMissing {
                model_id: crate::types::ModelId::from_static("llm-model"),
            }))
        } else {
            Ok(Arc::new(FakeTextPolisher::instant(FakePolish::Map(
                str::to_owned,
            ))))
        }
    };
    let chain = PolishChain::build_with(plan(&[RULES, LLM], false), None, build);
    assert_eq!(chain.stage_ids().collect::<Vec<_>>(), [&RULES]);
    for _ in 0..2 {
        let outcome = block_on(chain.run("text", &context(None)));
        assert_eq!(outcome.polisher_ids, [RULES]);
        assert!(matches!(
            reasons(&outcome.fallbacks).as_slice(),
            [(
                "fake-llm",
                PolishFallbackReason::Unavailable(AppError::ModelMissing { .. })
            )]
        ));
    }
    assert_eq!(builds.load(Ordering::SeqCst), 2);

    let rebuilt = PolishChain::build_with(chain.plan().clone(), Some(&chain), build);
    assert_eq!(
        builds.load(Ordering::SeqCst),
        3,
        "the built stage is reused, the missing one is tried again"
    );
    assert_eq!(rebuilt.plan(), chain.plan());
}

#[test]
fn prepare_reports_stages_that_cannot_get_ready() {
    let (chain, slow) = fake_chain(FakePolish::Map(exclaim), false);
    slow.fail_next_prepare(PortError::new(AppError::Polish));
    assert_eq!(
        reasons(&block_on(chain.prepare())),
        [("fake-llm", &PolishFallbackReason::Failed(AppError::Polish))]
    );
    assert!(block_on(chain.prepare()).is_empty());
    assert_eq!(slow.prepares(), 2);
}

fn registry_chain(settings: &crate::types::SettingsSnapshot) -> PolishChain {
    let ctx = BuildCtx {
        paths: AppPaths::new("data", "resources"),
    };
    PolishChain::build(polish_plan(settings), &ctx, None)
}

#[test]
fn the_registry_chain_cleans_joined_segments() {
    let settings = resolve([(
        keys::DICTIONARY,
        SettingValue::Pairs(StaticList::from(vec![TextPair {
            from: StaticStr::new("cloud code"),
            to: StaticStr::new("Claude Code"),
        }])),
    )]);
    let chain = registry_chain(&settings);
    let caps = parakeet_caps();
    let context = polish_context(&settings, &caps, None);
    let text = join_segments(["Um, so I use cloud code.", "the the tests pass"]);
    let outcome = block_on(chain.run(&text, &context));
    assert_eq!(outcome.text, "So I use Claude Code. The tests pass ");
    assert_eq!(outcome.polisher_ids, [RULE_POLISHER]);
    assert!(outcome.fallbacks.is_empty());
}

/// Grammar polish switched on before its model and runtime are installed: the registry builds the stage, the
/// stage says what is missing, and the take keeps the rule output (02 §8.3).
#[test]
fn grammar_polish_without_its_model_keeps_the_rule_output() {
    let settings = resolve([(keys::LLM_ENABLED, SettingValue::Bool(true))]);
    let chain = registry_chain(&settings);
    assert_eq!(
        chain.stage_ids().map(|id| id.as_str()).collect::<Vec<_>>(),
        [RULE_POLISHER.as_str(), QWEN]
    );
    let outcome = block_on(chain.run("Um, hello.", &context(None)));
    assert_eq!(outcome.text, "Hello. ");
    assert_eq!(outcome.polisher_ids, [RULE_POLISHER]);
    assert_eq!(
        reasons(&outcome.fallbacks),
        [(
            QWEN,
            &PolishFallbackReason::Failed(AppError::ModelMissing {
                model_id: crate::registry::models::LLAMA_CPP_VULKAN
            })
        )]
    );
}

/// 02 §6.2: the whole chain on a 2-minute take (about 330 words at 165 wpm) with a full 500-pair dictionary.
#[test]
fn the_chain_polishes_a_two_minute_transcript_within_budget() {
    const SENTENCES: [&str; 6] = [
        "um so i think the the plan for this week is to ship the cloud code integration",
        "uh we need to, you know, finish the tests and and update the docs.",
        "th- the team said b-but the review is on thursday",
        "i- i will send the notes to the product group after the meeting.",
        "so the main risk is the the deadline, er, which is very very close",
        "you know, we can move the launch if the numbers are not ready.",
    ];
    let segments: Vec<&str> = SENTENCES.iter().copied().cycle().take(24).collect();
    let text = join_segments(segments.iter().copied());
    let words = text.split_whitespace().count();
    assert!((300..=400).contains(&words), "{words} words");

    let mut pairs: Vec<TextPair> = (0..499)
        .map(|index| TextPair {
            from: StaticStr::from(format!("term{index} name")),
            to: StaticStr::from(format!("Term{index} Name")),
        })
        .collect();
    pairs.push(TextPair {
        from: StaticStr::new("cloud code"),
        to: StaticStr::new("Claude Code"),
    });
    let settings = resolve([(
        keys::DICTIONARY,
        SettingValue::Pairs(StaticList::from(pairs)),
    )]);
    let chain = registry_chain(&settings);
    let mut caps = parakeet_caps();
    caps.casing = false;
    let context = polish_context(&settings, &caps, None);

    // The runtime exists before the take; only the chain is timed (its first run, which compiles the dictionary).
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    let started = Instant::now();
    let outcome = runtime.block_on(chain.run(&text, &context));
    let elapsed = started.elapsed();

    assert!(outcome.text.contains("Claude Code"));
    assert!(!outcome.text.contains("the the"));
    assert!(!outcome.text.to_lowercase().contains(" um "));
    eprintln!("polish chain: {words} words, 500 dictionary pairs, {elapsed:?}");
    let budget = if cfg!(debug_assertions) {
        Duration::from_millis(100)
    } else {
        Duration::from_millis(5)
    };
    assert!(elapsed < budget, "{elapsed:?} for {words} words");
}
