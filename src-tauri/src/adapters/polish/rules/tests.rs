/*!
 * SOURCE OF TRUTH KEYWORDS: RulePolisher tests, rule chain table tests, cloud code dictionary test, filler test, numbers to digits test, casing skip test
 * WHAT:  Table-driven tests of the whole rule polisher: every stage together, with the context switches (fillers
 *        off, engine casing, a named or detected language) and the TextPolisher contract.
 * WHY:   Each stage file tests its own rules; these prove the fixed order composes (a filler removed before spacing
 *        folds its comma, a stutter collapsed before casing, a doubled number word kept by repeats for the numbers
 *        stage) and that the context flags reach the right stage.
 * WHERE: `cargo test` (adapters::polish::rules::tests).
 */

use std::{sync::Arc, task::Poll};

use super::*;
use crate::{
    ports::fakes::poll_once,
    types::{Language, StaticList, StaticStr},
};

fn pairs(list: &[(&str, &str)]) -> StaticList<TextPair> {
    StaticList::from(
        list.iter()
            .map(|(from, to)| TextPair {
                from: StaticStr::from((*from).to_owned()),
                to: StaticStr::from((*to).to_owned()),
            })
            .collect::<Vec<_>>(),
    )
}

/// An engine that punctuates; `cased` says whether it also cases.
fn context(cased: bool) -> PolishContext {
    PolishContext {
        language: None,
        punctuated: true,
        cased,
        remove_fillers: true,
        dictionary: pairs(&[("cloud code", "Claude Code")]),
    }
}

#[test]
fn every_stage_runs_in_order() {
    let polisher = RulePolisher::new();
    let uncased = context(false);
    let cased = context(true);
    let cases: &[(&PolishContext, &str, &str)] = &[
        (&uncased, "hello. world", "Hello. World"),
        (&uncased, "i think we should go", "I think we should go"),
        (&uncased, "the the cat sat", "The cat sat"),
        (&uncased, "um, so we start", "So we start"),
        (&cased, "Um, so we start.", "So we start."),
        (
            &uncased,
            "i use cloud code every day",
            "I use Claude Code every day",
        ),
        (&cased, "I use Cloud Code.", "I use Claude Code."),
        (&cased, "We should go, um.", "We should go."),
        (&cased, "You know, it works, you know.", "It works."),
        (
            &cased,
            "I- I think the the answer is, uh, forty two.",
            "I think the answer is 42.",
        ),
        (&cased, "Twenty, fifty.", "20, 50."),
        (
            &cased,
            "Um, it was built in fifteen twenty three, not one thousand five hundred.",
            "It was built in 1523, not 1500.",
        ),
        (
            &uncased,
            "i- i have twenty twenty five tickets at ten thirty",
            "I have 2025 tickets at 10:30",
        ),
        (
            &uncased,
            "twenty people came. five left",
            "20 people came. Five left",
        ),
        (
            &cased,
            "One of them had five apples.",
            "One of them had five apples.",
        ),
        (&cased, "Hello.  World , again", "Hello. World, again"),
        (&cased, "hello. world", "hello. world"),
        (&cased, "Um.", ""),
        (&cased, "", ""),
        (&uncased, "b-but i- i said so", "But I said so"),
    ];
    for (context, input, expected) in cases {
        assert_eq!(polisher.apply(input, context), *expected, "{input}");
    }
}

#[test]
fn fillers_stay_when_the_setting_is_off() {
    let polisher = RulePolisher::new();
    let context = PolishContext {
        remove_fillers: false,
        ..context(true)
    };
    assert_eq!(
        polisher.apply("Um, so we start.", &context),
        "Um, so we start."
    );
    assert_eq!(
        polisher.apply("the the end", &context),
        "the end",
        "repeats do not depend on the filler setting"
    );
}

#[test]
fn other_languages_keep_their_words() {
    let polisher = RulePolisher::new();
    let detected = context(true);
    assert_eq!(
        polisher.apply("Er ist um drei Uhr nicht da.", &detected),
        "Er ist um drei Uhr nicht da."
    );
    assert_eq!(
        polisher.apply("Um dia muito bom, obrigado.", &detected),
        "Um dia muito bom, obrigado."
    );
    assert_eq!(
        polisher.apply("Ich habe zwanzig Euro und twenty.", &detected),
        "Ich habe zwanzig Euro und twenty.",
        "number words are written as digits only for a language with number data"
    );
    let german = PolishContext {
        language: Some(Language::from_static("de")),
        ..context(true)
    };
    assert_eq!(polisher.apply("Um, äh, so.", &german), "Um, äh, so.");
    let italian = PolishContext {
        language: Some(Language::from_static("it")),
        ..context(false)
    };
    assert_eq!(polisher.apply("vedo i ragazzi", &italian), "Vedo i ragazzi");
}

#[test]
fn the_dictionary_is_compiled_once_per_pair_list() {
    let polisher = RulePolisher::new();
    let first = polisher.dictionary(&context(true).dictionary);
    let again = polisher.dictionary(&context(true).dictionary);
    assert!(Arc::ptr_eq(&first, &again));
    let changed = polisher.dictionary(&pairs(&[("echo", "Echo")]));
    assert!(!Arc::ptr_eq(&first, &changed));
    assert_eq!(changed.apply("echo"), "Echo");
}

#[test]
fn the_port_is_instant_ready_and_never_fails() {
    let polisher = RulePolisher::new();
    assert_eq!(polisher.caps(), RulePolisher::CAPS);
    assert!(!polisher.caps().needs_model);
    assert!(matches!(poll_once(polisher.prepare()), Poll::Ready(Ok(()))));
    let context = context(true);
    assert!(matches!(
        poll_once(polisher.polish("Um, the the plan.", &context)),
        Poll::Ready(Ok(text)) if text == "The plan."
    ));
}
