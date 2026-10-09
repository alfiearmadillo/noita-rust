//! Animation stack semantics recovered from `SpriteAnimatorSystem::PlayAnimation` (0xc1bf70).

use actors::{AnimationSet, Animator};

struct Has(&'static [&'static str]);

impl AnimationSet for Has {
    fn has(&self, name: &str) -> bool {
        self.0.contains(&name)
    }
}

const SPRITE: Has = Has(&["stand", "walk", "land", "eat"]);

fn names(a: &Animator) -> Vec<(&str, i32)> {
    a.states
        .iter()
        .map(|s| (s.name.as_str(), s.priority))
        .collect()
}

#[test]
fn same_priority_replaces_and_higher_stacks_on_top() {
    let mut a = Animator::default();
    assert!(a.play(&SPRITE, "stand", 0, ""));
    assert!(a.play(&SPRITE, "land", 1, ""));
    assert!(a.play(&SPRITE, "walk", 0, ""));
    assert_eq!(names(&a), [("walk", 0), ("land", 1)]);
    assert_eq!(a.current(), Some("land"));
}

#[test]
fn missing_names_use_the_fallback_at_priority_zero() {
    let mut a = Animator::default();
    assert!(a.play(&SPRITE, "swim_move", 3, "walk"));
    assert_eq!(names(&a), [("walk", 0)]);
    assert!(!a.play(&SPRITE, "fly_move_backwards", 0, "fly_move"));
    assert!(!a.play(&SPRITE, "nope", 0, ""));
}

#[test]
fn a_name_already_on_the_stack_is_not_restarted() {
    let mut a = Animator::default();
    a.play(&SPRITE, "stand", 0, "");
    a.play(&SPRITE, "land", 1, "");
    assert!(a.play(&SPRITE, "stand", 0, ""));
    assert_eq!(names(&a), [("stand", 0), ("land", 1)]);
}

#[test]
fn lower_priority_than_everything_goes_to_the_bottom() {
    let mut a = Animator::default();
    a.play(&SPRITE, "land", 1, "");
    a.play(&SPRITE, "stand", 0, "");
    assert_eq!(names(&a), [("stand", 0), ("land", 1)]);
}

#[test]
fn stop_removes_by_name() {
    let mut a = Animator::default();
    a.play(&SPRITE, "stand", 0, "");
    a.play(&SPRITE, "eat", 2, "");
    a.stop(&SPRITE, "eat");
    assert_eq!(names(&a), [("stand", 0)]);
}

#[test]
fn finishing_pops_to_the_followup_or_next_animation() {
    let mut a = Animator::default();
    a.play(&SPRITE, "walk", 0, "");
    a.play_then(&SPRITE, "land", 1, "", "stand");
    a.finished(&SPRITE, "");
    assert_eq!(
        names(&a),
        [("stand", 0)],
        "follow-up replaces walk at priority 0"
    );
    a.finished(&SPRITE, "walk");
    assert_eq!(names(&a), [("walk", 0)], "no follow-up: next_animation");
    a.finished(&SPRITE, "");
    assert!(a.states.is_empty());
}
