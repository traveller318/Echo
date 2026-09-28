/*!
 * SOURCE OF TRUTH KEYWORDS: ChordTracker, KeyEvent, Reaction, HookBinding, chord matching, modifier-only hotkey, key repeat, stuck key reconcile, swallow key, lost key-up, menu mask, stale main key, stuck media key
 * WHAT:  ChordTracker: the pure state machine behind the low-level keyboard hook. Fed every physical key event
 *        (`handle`) and, while keys are held, a periodic check of which keys Windows still reports down
 *        (`reconcile`), it returns a Reaction: the HotkeyEvents to emit, whether to swallow the key, and whether to
 *        tap the menu mask key.
 * WHY:   The hook callback must stay trivial (Windows removes a hook that exceeds `LowLevelHooksTimeout`, 05 W9), and
 *        chord logic is easy to get subtly wrong, so it lives here with no Windows calls and is table-tested.
 *        Rules:
 *        - A chord with a main key fires when that key goes down while exactly its modifiers are held (as
 *          RegisterHotKey does) and no other main key is; the main key is swallowed for the whole press (repeats
 *          and key-up included), so the app in front never sees it. Its modifiers are never swallowed.
 *        - A modifier-only chord fires on the modifier key-down that makes the held set exactly its modifiers with
 *          no main key held; nothing is swallowed (the keys may still be meant for another shortcut). When Alt or
 *          Win is part of it, the caller taps the mask key so their release opens no menu bar or Start menu.
 *        - Another key joining a held modifier-only chord (a main key, or another modifier) reports Interrupted:
 *          the press was the start of another shortcut (Ctrl+Alt+T). A main key that itself fires a binding with
 *          fewer modifiers than are held (Esc while Ctrl+Alt is held for hold-to-talk) does not interrupt, so a take
 *          can be cancelled while its keys are held.
 *        - A chord is Released when its main key or any of its modifiers comes up; both sides of a modifier count
 *          as that modifier, so letting go of one side while the other is held keeps the chord.
 *        - The hook sees no auto-repeat flag, so a key-down for a key already held is a repeat when it comes within
 *          REPEAT_WINDOW_MS of the previous one (the slowest Windows repeat delay is 1 s); a later one means its
 *          key-up was lost (secure desktop, a hook timeout) and it is treated as a release and a fresh press.
 *        - Key-ups can be lost entirely (Ctrl+Alt+Del switches to the secure desktop, an elevated window takes the
 *          focus, Windows skips a hook that answered too slowly), which would leave a hold-to-talk take recording
 *          forever: `reconcile` releases a held key that Windows has reported up on two checks in a row. Swallowed
 *          keys are skipped there, because Windows never records their state.
 *        - A swallowed key never reached any app, so it never counts as "another key held" against a chord. A
 *          swallowed key whose key-up was lost (Esc during a take, the V of Ctrl+Alt+V) would otherwise stay held
 *          with nothing to heal it until that same key is pressed again, and every chord would stay dead meanwhile:
 *          the "record hotkey stops working until Echo restarts" failure.
 *        - A main key physically held auto-repeats, so one whose last down is older than REPEAT_WINDOW_MS is stale:
 *          its key-up was lost where Windows' own key state cannot vouch for it. A Bluetooth headset's media key is
 *          the case that forced this: its AVRCP button sends Play/Pause down with no up, Windows then reports that
 *          key down for good, and `reconcile` alone kept it held, so every chord stayed dead until Echo restarted.
 *          A stale main key never blocks a chord and `reconcile` lets it go (reporting it in `Reaction::lost`).
 *          Modifiers are exempt: while a chord is held only the last key pressed repeats.
 *        - An event for a binding that was unregistered or rebound meanwhile is dropped.
 * WHERE: Owned by the hook thread (hook_thread.rs), one per LowLevelKeyboardHotkeys; bindings come from the
 *        adapter's table on every event.
 */

use std::collections::BTreeMap;

use super::chord::{Chord, KeyKind, Modifiers, key_of};
use crate::types::{HotkeyEvent, HotkeyId, KeyState};

/// A key-down this soon after the previous down of the same key is an auto-repeat, in ms.
pub const REPEAT_WINDOW_MS: u32 = 1_100;

/// Consecutive reconcile checks that must see a held key up before it is released.
const STALE_STRIKES: u8 = 2;

/// One physical key event from the hook (time in GetTickCount milliseconds, which wrap).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub vk: u16,
    pub down: bool,
    pub time: u32,
}

/// A bound chord.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookBinding {
    pub id: HotkeyId,
    pub chord: Chord,
}

/// What the hook does with one event.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Reaction {
    pub events: Vec<HotkeyEvent>,
    /// The key must not reach the app in front.
    pub swallow: bool,
    /// Tap the menu mask key once this event has passed.
    pub mask: bool,
    /// Held keys `reconcile` let go because their key-up never arrived (for the log).
    pub lost: Vec<u16>,
}

#[derive(Debug, Clone, Copy)]
struct HeldKey {
    /// When its last down (or repeat) arrived.
    last_down: u32,
    swallowed: bool,
    /// Reconcile checks in a row that saw it up.
    strikes: u8,
}

impl HeldKey {
    /// A main key that has not repeated for REPEAT_WINDOW_MS at `now` is no longer held (module docs).
    fn is_stale_main(&self, vk: u16, now: u32) -> bool {
        key_of(vk) == KeyKind::Main && elapsed(self.last_down, now) > REPEAT_WINDOW_MS
    }
}

/// Milliseconds from `earlier` to `later` on the wrapping tick clock; 0 when `later` is in fact the earlier one
/// (a timer message stamped a tick before the key event it follows).
fn elapsed(earlier: u32, later: u32) -> u32 {
    let forward = later.wrapping_sub(earlier);
    if forward > u32::MAX / 2 { 0 } else { forward }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Active {
    id: HotkeyId,
    chord: Chord,
}

/// The keys held and the chords currently down.
#[derive(Debug, Default)]
pub struct ChordTracker {
    held: BTreeMap<u16, HeldKey>,
    active: Vec<Active>,
}

impl ChordTracker {
    /// Some key the tracker believes held could have lost its key-up (so `reconcile` should run).
    pub fn needs_reconcile(&self) -> bool {
        self.held.values().any(|key| !key.swallowed)
    }

    /// Reacts to one physical key event under `bindings`.
    pub fn handle(&mut self, event: KeyEvent, bindings: &[HookBinding]) -> Reaction {
        let mut reaction = Reaction::default();
        self.forget_unbound(bindings);
        if event.down {
            self.key_down(event, bindings, &mut reaction);
        } else {
            reaction.swallow = self.key_up(event.vk, &mut reaction.events);
        }
        reaction
    }

    /// Releases every held key that `is_down` has reported up on STALE_STRIKES checks in a row, and every main key
    /// stale at `now` (GetTickCount ms) whatever `is_down` says.
    pub fn reconcile(
        &mut self,
        now: u32,
        is_down: impl Fn(u16) -> bool,
        bindings: &[HookBinding],
    ) -> Reaction {
        let mut reaction = Reaction::default();
        self.forget_unbound(bindings);
        let mut stale = Vec::new();
        for (vk, key) in &mut self.held {
            if key.swallowed {
                continue;
            }
            if key.is_stale_main(*vk, now) {
                stale.push(*vk);
            } else if is_down(*vk) {
                key.strikes = 0;
            } else {
                key.strikes = key.strikes.saturating_add(1);
                if key.strikes >= STALE_STRIKES {
                    stale.push(*vk);
                }
            }
        }
        for vk in stale {
            self.key_up(vk, &mut reaction.events);
            reaction.lost.push(vk);
        }
        reaction
    }

    fn key_down(&mut self, event: KeyEvent, bindings: &[HookBinding], reaction: &mut Reaction) {
        if let Some(key) = self.held.get_mut(&event.vk) {
            if elapsed(key.last_down, event.time) <= REPEAT_WINDOW_MS {
                key.last_down = event.time;
                reaction.swallow = key.swallowed;
                return;
            }
            // Its key-up was lost: release it before treating this as a fresh press.
            self.key_up(event.vk, &mut reaction.events);
        }
        self.held.insert(
            event.vk,
            HeldKey {
                last_down: event.time,
                swallowed: false,
                strikes: 0,
            },
        );
        match key_of(event.vk) {
            KeyKind::Main => self.main_down(event, bindings, reaction),
            KeyKind::Modifier(modifier) => {
                self.modifier_down(modifier, event.time, bindings, reaction);
            }
        }
        if reaction.swallow
            && let Some(key) = self.held.get_mut(&event.vk)
        {
            key.swallowed = true;
        }
    }

    fn main_down(&mut self, event: KeyEvent, bindings: &[HookBinding], reaction: &mut Reaction) {
        let vk = event.vk;
        let held = self.held_modifiers();
        let lone_main = self.blocking_main_keys(event.time).all(|other| other == vk);
        let inactive = |binding: &&HookBinding| {
            binding.chord.key == Some(vk) && !self.active.iter().any(|a| a.id == binding.id)
        };
        let exact = bindings
            .iter()
            .filter(inactive)
            .find(|binding| lone_main && binding.chord.modifiers == held);
        // Keys held for a modifier-only chord do not count against a binding with fewer modifiers (Esc during
        // a hold-to-talk take).
        let within_held_chord = || {
            bindings.iter().filter(inactive).find(|binding| {
                lone_main
                    && self.active.iter().any(|active| {
                        active.chord.is_modifier_only()
                            && binding.chord.modifiers == held - active.chord.modifiers
                    })
            })
        };
        match exact {
            Some(binding) => {
                self.interrupt_modifier_only(|_| true, &mut reaction.events);
                self.activate(binding, reaction);
                reaction.swallow = true;
            }
            None => match within_held_chord() {
                Some(binding) => {
                    self.activate(binding, reaction);
                    reaction.swallow = true;
                }
                None => self.interrupt_modifier_only(|_| true, &mut reaction.events),
            },
        }
    }

    fn modifier_down(
        &mut self,
        modifier: Modifiers,
        now: u32,
        bindings: &[HookBinding],
        reaction: &mut Reaction,
    ) {
        self.interrupt_modifier_only(
            |chord| !chord.modifiers.contains(modifier),
            &mut reaction.events,
        );
        if self.blocking_main_keys(now).next().is_some() {
            return;
        }
        let held = self.held_modifiers();
        let fired = bindings.iter().find(|binding| {
            binding.chord.is_modifier_only()
                && binding.chord.modifiers == held
                && !self.active.iter().any(|active| active.id == binding.id)
        });
        if let Some(binding) = fired {
            reaction.mask |= binding.chord.modifiers.opens_menus();
            self.activate(binding, reaction);
        }
    }

    /// Removes `vk` and releases every chord that no longer holds; true when the key was swallowed on its way down.
    fn key_up(&mut self, vk: u16, events: &mut Vec<HotkeyEvent>) -> bool {
        let swallowed = self.held.remove(&vk).is_some_and(|key| key.swallowed);
        let held = self.held_modifiers();
        let held_keys = &self.held;
        self.active.retain(|active| {
            let main_held = active
                .chord
                .key
                .is_none_or(|key| held_keys.contains_key(&key));
            let holds = main_held && held.contains(active.chord.modifiers);
            if !holds {
                events.push(HotkeyEvent {
                    id: active.id.clone(),
                    state: KeyState::Released,
                });
            }
            holds
        });
        swallowed
    }

    fn activate(&mut self, binding: &HookBinding, reaction: &mut Reaction) {
        self.active.push(Active {
            id: binding.id.clone(),
            chord: binding.chord,
        });
        reaction.events.push(HotkeyEvent {
            id: binding.id.clone(),
            state: KeyState::Pressed,
        });
    }

    /// Reports Interrupted for, and drops, every active modifier-only chord `joined` accepts.
    fn interrupt_modifier_only(
        &mut self,
        joined: impl Fn(&Chord) -> bool,
        events: &mut Vec<HotkeyEvent>,
    ) {
        self.active.retain(|active| {
            let interrupted = active.chord.is_modifier_only() && joined(&active.chord);
            if interrupted {
                events.push(HotkeyEvent {
                    id: active.id.clone(),
                    state: KeyState::Interrupted,
                });
            }
            !interrupted
        });
    }

    /// Drops active chords whose binding was removed or rebound; their keys finish without events.
    fn forget_unbound(&mut self, bindings: &[HookBinding]) {
        self.active.retain(|active| {
            bindings
                .iter()
                .any(|binding| binding.id == active.id && binding.chord == active.chord)
        });
    }

    fn held_modifiers(&self) -> Modifiers {
        self.held
            .keys()
            .fold(Modifiers::NONE, |held, vk| match key_of(*vk) {
                KeyKind::Modifier(modifier) => held | modifier,
                KeyKind::Main => held,
            })
    }

    /// Held main keys an app saw go down: they keep a chord from firing. Swallowed ones and ones stale at `now` are
    /// left out (module docs).
    fn blocking_main_keys(&self, now: u32) -> impl Iterator<Item = u16> + '_ {
        self.held
            .iter()
            .filter(move |(vk, key)| {
                !key.swallowed && key_of(**vk) == KeyKind::Main && !key.is_stale_main(**vk, now)
            })
            .map(|(vk, _)| *vk)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{adapters::hotkey::low_level_hook::chord::parse, types::Shortcut};

    const RECORD: HotkeyId = HotkeyId::from_static("record");
    const CANCEL: HotkeyId = HotkeyId::from_static("cancel");
    const PASTE: HotkeyId = HotkeyId::from_static("paste-last");

    const LCTRL: u16 = 0xA2;
    const RCTRL: u16 = 0xA3;
    const LALT: u16 = 0xA4;
    const LSHIFT: u16 = 0xA0;
    const SPACE: u16 = 0x20;
    const ESC: u16 = 0x1B;
    const T: u16 = 0x54;
    const V: u16 = 0x56;
    const MEDIA_PLAY_PAUSE: u16 = 0xB3;

    fn binding(id: HotkeyId, text: &'static str) -> HookBinding {
        HookBinding {
            id,
            chord: parse(&Shortcut::from_static(text)).unwrap(),
        }
    }

    /// Drives a tracker with a clock that advances 10 ms per event.
    struct Keyboard {
        tracker: ChordTracker,
        bindings: Vec<HookBinding>,
        now: u32,
    }

    impl Keyboard {
        fn new(bindings: Vec<HookBinding>) -> Self {
            Self {
                tracker: ChordTracker::default(),
                bindings,
                now: 1_000,
            }
        }

        fn key(&mut self, vk: u16, down: bool) -> Reaction {
            self.now = self.now.wrapping_add(10);
            self.tracker.handle(
                KeyEvent {
                    vk,
                    down,
                    time: self.now,
                },
                &self.bindings,
            )
        }

        fn down(&mut self, vk: u16) -> Reaction {
            self.key(vk, true)
        }

        fn up(&mut self, vk: u16) -> Reaction {
            self.key(vk, false)
        }

        fn wait(&mut self, ms: u32) {
            self.now = self.now.wrapping_add(ms);
        }
    }

    fn event(id: HotkeyId, state: KeyState) -> HotkeyEvent {
        HotkeyEvent { id, state }
    }

    fn only(id: HotkeyId, state: KeyState) -> Vec<HotkeyEvent> {
        vec![event(id, state)]
    }

    #[test]
    fn a_modifier_only_chord_fires_when_held_and_releases_on_the_first_key_up() {
        let mut keys = Keyboard::new(vec![binding(RECORD, "Ctrl+Alt")]);
        assert_eq!(keys.down(LCTRL), Reaction::default());
        let pressed = keys.down(LALT);
        assert_eq!(pressed.events, only(RECORD, KeyState::Pressed));
        assert!(!pressed.swallow, "the modifiers still reach the app");
        assert!(
            pressed.mask,
            "Alt is in the chord: its release must open no menu"
        );
        // Auto-repeat of the held keys changes nothing.
        assert_eq!(keys.down(LALT), Reaction::default());
        assert_eq!(keys.down(LCTRL), Reaction::default());
        assert_eq!(keys.up(LALT).events, only(RECORD, KeyState::Released));
        assert_eq!(keys.up(LCTRL), Reaction::default());
        // Pressing it again is a new press.
        keys.down(LALT);
        assert_eq!(keys.down(LCTRL).events, only(RECORD, KeyState::Pressed));
    }

    #[test]
    fn another_key_joining_a_modifier_only_chord_interrupts_it() {
        let mut keys = Keyboard::new(vec![binding(RECORD, "Ctrl+Alt")]);
        keys.down(LCTRL);
        keys.down(LALT);
        let joined = keys.down(T);
        assert_eq!(joined.events, only(RECORD, KeyState::Interrupted));
        assert!(!joined.swallow, "the other shortcut still reaches the app");
        assert_eq!(keys.up(T), Reaction::default());
        assert_eq!(
            keys.up(LALT),
            Reaction::default(),
            "no Released after Interrupted"
        );
        keys.up(LCTRL);

        keys.down(LCTRL);
        keys.down(LALT);
        assert_eq!(
            keys.down(LSHIFT).events,
            only(RECORD, KeyState::Interrupted),
            "a third modifier is another shortcut too"
        );
    }

    #[test]
    fn a_held_main_key_or_extra_modifier_keeps_a_modifier_only_chord_from_firing() {
        let mut keys = Keyboard::new(vec![binding(RECORD, "Ctrl+Alt")]);
        keys.down(T);
        keys.down(LCTRL);
        assert_eq!(keys.down(LALT), Reaction::default());
        keys.up(T);
        keys.up(LALT);
        keys.up(LCTRL);
        keys.down(LSHIFT);
        keys.down(LCTRL);
        assert_eq!(keys.down(LALT), Reaction::default());
    }

    #[test]
    fn both_sides_of_a_modifier_hold_the_chord() {
        let mut keys = Keyboard::new(vec![binding(RECORD, "Ctrl+Alt")]);
        keys.down(LCTRL);
        keys.down(LALT);
        assert_eq!(keys.down(RCTRL), Reaction::default());
        assert_eq!(
            keys.up(LCTRL),
            Reaction::default(),
            "the right Ctrl still holds it"
        );
        assert_eq!(keys.up(RCTRL).events, only(RECORD, KeyState::Released));
    }

    #[test]
    fn a_chord_with_a_main_key_fires_on_that_key_and_swallows_it() {
        let mut keys = Keyboard::new(vec![binding(RECORD, "Ctrl+Alt+Space")]);
        keys.down(LCTRL);
        assert_eq!(keys.down(LALT), Reaction::default());
        let pressed = keys.down(SPACE);
        assert_eq!(pressed.events, only(RECORD, KeyState::Pressed));
        assert!(pressed.swallow);
        assert!(!pressed.mask);
        let repeat = keys.down(SPACE);
        assert!(repeat.swallow && repeat.events.is_empty());
        let released = keys.up(SPACE);
        assert_eq!(released.events, only(RECORD, KeyState::Released));
        assert!(released.swallow);
        // Wrong modifiers: nothing fires and the key passes through.
        keys.up(LALT);
        let plain = keys.down(SPACE);
        assert_eq!(plain, Reaction::default());
    }

    #[test]
    fn releasing_a_modifier_releases_a_chord_whose_main_key_is_still_held() {
        let mut keys = Keyboard::new(vec![binding(RECORD, "Ctrl+Alt+Space")]);
        keys.down(LCTRL);
        keys.down(LALT);
        keys.down(SPACE);
        assert_eq!(keys.up(LCTRL).events, only(RECORD, KeyState::Released));
        let up = keys.up(SPACE);
        assert!(
            up.swallow,
            "a swallowed key-down keeps its key-up swallowed"
        );
        assert!(up.events.is_empty());
    }

    #[test]
    fn escape_fires_while_a_hold_to_talk_chord_is_held_without_interrupting_it() {
        let mut keys = Keyboard::new(vec![binding(RECORD, "Ctrl+Alt"), binding(CANCEL, "Escape")]);
        keys.down(LCTRL);
        keys.down(LALT);
        let esc = keys.down(ESC);
        assert_eq!(esc.events, only(CANCEL, KeyState::Pressed));
        assert!(esc.swallow);
        assert_eq!(keys.up(ESC).events, only(CANCEL, KeyState::Released));
        assert_eq!(keys.up(LALT).events, only(RECORD, KeyState::Released));
    }

    #[test]
    fn a_shortcut_that_extends_the_held_chord_fires_and_interrupts_it() {
        let mut keys = Keyboard::new(vec![
            binding(RECORD, "Ctrl+Alt"),
            binding(PASTE, "Ctrl+Alt+V"),
        ]);
        keys.down(LCTRL);
        keys.down(LALT);
        let paste = keys.down(V);
        assert_eq!(
            paste.events,
            vec![
                event(RECORD, KeyState::Interrupted),
                event(PASTE, KeyState::Pressed)
            ]
        );
        assert!(paste.swallow);
    }

    #[test]
    fn a_lost_key_up_is_healed_by_a_later_press_or_by_reconcile() {
        let mut keys = Keyboard::new(vec![binding(CANCEL, "Escape")]);
        assert_eq!(keys.down(ESC).events, only(CANCEL, KeyState::Pressed));
        // The key-up never arrives; a press long after is a new press, not a repeat.
        keys.wait(REPEAT_WINDOW_MS + 1);
        assert_eq!(
            keys.down(ESC).events,
            vec![
                event(CANCEL, KeyState::Released),
                event(CANCEL, KeyState::Pressed)
            ]
        );

        let mut keys = Keyboard::new(vec![binding(RECORD, "Ctrl+Alt")]);
        keys.down(LCTRL);
        keys.down(LALT);
        assert!(keys.tracker.needs_reconcile());
        let bindings = keys.bindings.clone();
        let all_up = |_: u16| false;
        assert_eq!(
            keys.tracker.reconcile(keys.now, all_up, &bindings),
            Reaction::default(),
            "one check is not enough"
        );
        assert_eq!(
            keys.tracker.reconcile(keys.now, all_up, &bindings).events,
            only(RECORD, KeyState::Released)
        );
        assert!(!keys.tracker.needs_reconcile());
    }

    #[test]
    fn reconcile_keeps_keys_that_are_still_down_and_skips_swallowed_ones() {
        let mut keys = Keyboard::new(vec![binding(CANCEL, "Escape")]);
        keys.down(ESC);
        assert!(
            !keys.tracker.needs_reconcile(),
            "Windows never records a swallowed key"
        );
        let bindings = keys.bindings.clone();
        for _ in 0..3 {
            assert_eq!(
                keys.tracker.reconcile(keys.now, |_| false, &bindings),
                Reaction::default()
            );
        }
        keys.down(LCTRL);
        let flicker = [false, true, false];
        for seen_down in flicker {
            assert_eq!(
                keys.tracker.reconcile(keys.now, |_| seen_down, &bindings),
                Reaction::default()
            );
        }
    }

    #[test]
    fn an_unbound_or_rebound_chord_goes_quiet() {
        let mut keys = Keyboard::new(vec![binding(RECORD, "Ctrl+Alt")]);
        keys.down(LCTRL);
        keys.down(LALT);
        keys.bindings = vec![binding(RECORD, "Ctrl+Shift")];
        assert_eq!(keys.up(LALT), Reaction::default());
        keys.bindings.clear();
        keys.down(LALT);
        assert_eq!(keys.down(T), Reaction::default());
    }

    #[test]
    fn a_swallowed_key_that_lost_its_key_up_never_blocks_the_record_chord() {
        let mut keys = Keyboard::new(vec![
            binding(RECORD, "Ctrl+Alt"),
            binding(CANCEL, "Escape"),
            binding(PASTE, "Ctrl+Alt+V"),
        ]);
        // Esc is swallowed, and its key-up never arrives (a hook timeout, the secure desktop).
        assert!(keys.down(ESC).swallow);
        keys.wait(5_000);
        keys.down(LCTRL);
        assert_eq!(
            keys.down(LALT).events,
            only(RECORD, KeyState::Pressed),
            "the lost Esc must not keep Ctrl+Alt dead"
        );
        keys.up(LALT);
        keys.up(LCTRL);

        // The same for the swallowed main key of another chord.
        keys.down(LCTRL);
        keys.down(LALT);
        assert!(keys.down(V).swallow);
        keys.up(LALT);
        keys.up(LCTRL);
        keys.wait(5_000);
        keys.down(LCTRL);
        assert_eq!(keys.down(LALT).events, only(RECORD, KeyState::Pressed));
        keys.up(LALT);
        keys.up(LCTRL);
        // Pressing the lost key again heals it and fires its chord again.
        keys.down(LCTRL);
        keys.down(LALT);
        let paste = keys.down(V);
        assert!(paste.events.contains(&event(PASTE, KeyState::Pressed)));
        assert!(paste.swallow);
    }

    #[test]
    fn a_main_key_an_app_saw_still_blocks_the_record_chord() {
        let mut keys = Keyboard::new(vec![binding(RECORD, "Ctrl+Alt"), binding(CANCEL, "Escape")]);
        keys.down(T);
        keys.down(LCTRL);
        assert_eq!(keys.down(LALT), Reaction::default());
    }

    /// The live failure of 2026-09-28: Bluetooth earbuds sent Play/Pause down with no up, and Windows reported it
    /// down from then on, so neither a key-up nor `reconcile` ever let it go.
    #[test]
    fn a_device_key_windows_keeps_down_never_blocks_the_record_chord() {
        let mut keys = Keyboard::new(vec![
            binding(RECORD, "Ctrl+Alt"),
            binding(PASTE, "Ctrl+Alt+V"),
        ]);
        let bindings = keys.bindings.clone();
        let windows_says_down = |vk: u16| vk == MEDIA_PLAY_PAUSE;
        assert_eq!(keys.down(MEDIA_PLAY_PAUSE), Reaction::default());
        assert!(keys.tracker.needs_reconcile());
        assert!(
            keys.tracker
                .reconcile(keys.now, windows_says_down, &bindings)
                .lost
                .is_empty(),
            "a key that just went down is still held"
        );

        keys.wait(REPEAT_WINDOW_MS + 1);
        keys.down(LCTRL);
        assert_eq!(
            keys.down(LALT).events,
            only(RECORD, KeyState::Pressed),
            "a key that stopped repeating is no longer held"
        );
        assert_eq!(
            keys.down(V).events.last(),
            Some(&event(PASTE, KeyState::Pressed))
        );
        keys.up(V);
        keys.up(LALT);
        keys.up(LCTRL);

        let healed = keys
            .tracker
            .reconcile(keys.now, windows_says_down, &bindings);
        assert_eq!(healed.lost, [MEDIA_PLAY_PAUSE]);
        assert!(healed.events.is_empty());
        assert!(
            !keys.tracker.needs_reconcile(),
            "nothing left to watch, so the reconcile timer stops"
        );
    }

    #[test]
    fn a_timer_stamped_before_the_last_key_event_sees_no_stale_key() {
        let mut keys = Keyboard::new(vec![binding(RECORD, "Ctrl+Alt")]);
        keys.down(T);
        let bindings = keys.bindings.clone();
        let earlier = keys.now.wrapping_sub(5);
        assert!(
            keys.tracker
                .reconcile(earlier, |_| true, &bindings)
                .lost
                .is_empty()
        );
        keys.down(LCTRL);
        assert_eq!(keys.down(LALT), Reaction::default(), "T is still held");
    }

    /// While a renewed hook and the one it replaced are both installed, an unswallowed key reaches the tracker twice
    /// with the same timestamp (hook_thread.rs `Hooks`); the second copy must change nothing.
    #[test]
    fn a_duplicated_event_changes_nothing() {
        let mut keys = Keyboard::new(vec![binding(RECORD, "Ctrl+Alt")]);
        let mut twice = |vk: u16, down: bool| {
            keys.now = keys.now.wrapping_add(10);
            let event = KeyEvent {
                vk,
                down,
                time: keys.now,
            };
            let first = keys.tracker.handle(event, &keys.bindings);
            let second = keys.tracker.handle(event, &keys.bindings);
            (first, second)
        };
        assert_eq!(twice(LCTRL, true).1, Reaction::default());
        let (pressed, again) = twice(LALT, true);
        assert_eq!(pressed.events, only(RECORD, KeyState::Pressed));
        assert!(pressed.mask);
        assert_eq!(
            again,
            Reaction::default(),
            "no second press and no second mask tap"
        );
        let (interrupted, again) = twice(T, true);
        assert_eq!(interrupted.events, only(RECORD, KeyState::Interrupted));
        assert_eq!(again, Reaction::default());
        assert_eq!(twice(T, false).1, Reaction::default());
        assert_eq!(twice(LALT, false).1, Reaction::default());
        assert_eq!(twice(LCTRL, false).1, Reaction::default());
        // The chord still works afterwards.
        keys.down(LCTRL);
        assert_eq!(keys.down(LALT).events, only(RECORD, KeyState::Pressed));
    }

    #[test]
    fn repeats_are_recognised_across_the_tick_counter_wrap() {
        let mut keys = Keyboard::new(vec![binding(CANCEL, "Escape")]);
        keys.now = u32::MAX - 5;
        assert_eq!(keys.down(ESC).events, only(CANCEL, KeyState::Pressed));
        let repeat = keys.down(ESC);
        assert!(repeat.events.is_empty() && repeat.swallow);
    }
}
