//! `SpriteAnimatorComponent.mStates` and `SpriteAnimatorSystem::PlayAnimation` (0xc1bf70,
//! name resolution 0xc1c350) / `StopAnimation` (0xc1c1d0) / finished-state handling (0xc1b820).

/// The target sprite's animations.
pub trait AnimationSet {
    fn has(&self, name: &str) -> bool;

    /// The sprite's current `rect_animation`.
    fn rect_animation(&self) -> &str {
        ""
    }
}

/// A sprite with no animations; every play request fails.
impl AnimationSet for () {
    fn has(&self, _: &str) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnimState {
    pub name: String,
    pub priority: i32,
    /// Played when this state finishes; empty means the finished animation's `next_animation`.
    pub followup: String,
    pub followup_priority: i32,
}

/// Stack of animation states ordered by priority; the last state is the one shown.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Animator {
    pub states: Vec<AnimState>,
}

impl Animator {
    /// Plays `name` at `priority`, or `fallback` (priority 0) if the sprite lacks `name`.
    /// Returns false only when neither exists. A name already on the stack is left alone.
    pub fn play(
        &mut self,
        sprite: &dyn AnimationSet,
        name: &str,
        priority: i32,
        fallback: &str,
    ) -> bool {
        self.play_then(sprite, name, priority, fallback, "")
    }

    /// [`Animator::play`] with a follow-up animation (priority 0) for when this one finishes.
    pub fn play_then(
        &mut self,
        sprite: &dyn AnimationSet,
        name: &str,
        priority: i32,
        fallback: &str,
        followup: &str,
    ) -> bool {
        let (name, priority) = if sprite.has(name) {
            (name, priority)
        } else if !fallback.is_empty() && sprite.has(fallback) {
            (fallback, 0)
        } else {
            return false;
        };
        if self.states.iter().any(|s| s.name == name) {
            return true;
        }
        let state = AnimState {
            name: name.to_owned(),
            priority,
            followup: followup.to_owned(),
            followup_priority: 0,
        };
        let slot = self.states.iter().rposition(|s| priority >= s.priority);
        match slot {
            _ if self.states.is_empty() => self.states.push(state),
            None => self.states.insert(0, state),
            Some(i) if self.states[i].priority == priority => self.states[i] = state,
            Some(i) => self.states.insert(i + 1, state),
        }
        true
    }

    /// Removes the first state named `name`, if the sprite has that animation.
    pub fn stop(&mut self, sprite: &dyn AnimationSet, name: &str) {
        if sprite.has(name)
            && let Some(i) = self.states.iter().position(|s| s.name == name)
        {
            self.states.remove(i);
        }
    }

    /// The sprite's current animation finished: pop the top state and play its follow-up, or the
    /// finished animation's `next_animation` when it has none.
    pub fn finished(&mut self, sprite: &dyn AnimationSet, next_animation: &str) {
        let Some(top) = self.states.pop() else {
            return;
        };
        let name = if top.followup.is_empty() {
            next_animation
        } else {
            &top.followup
        };
        if !name.is_empty() {
            self.play(sprite, name, top.followup_priority, "");
        }
    }

    pub fn current(&self) -> Option<&str> {
        self.states.last().map(|s| s.name.as_str())
    }
}
