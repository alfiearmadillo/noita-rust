//! The wand engine: the reference's own `gun.lua` runs unchanged and the C++ half it talks to
//! (`RegisterGunAction`, `BeginProjectile`, `EndTrigger`, ... at `0xb27ff0..0xb28d80`) records a
//! command list, as the engine does (records of `0x25c` bytes appended at `0x12093f8`). The host
//! reads the commands back to spawn projectiles.
//!
//! Not modelled: the progress flags `OnActionPlayed` sets (`0xb28b10`), the per-item use counts
//! `ActionUsesRemainingChanged` updates (`0xb28930`; it returns true unless the game rejects the
//! change) and `ActionUsed`'s statistics.

use mlua::{Function, Lua, MultiValue, Table, Value};

use crate::{Runtime, ScriptState};

/// Argument order of `RegisterGunAction` (`gunaction_generated.lua`, `ConfigGunActionInfo_*`).
pub const ACTION_INFO_FIELDS: [&str; 65] = [
    "action_id",
    "action_name",
    "action_description",
    "action_sprite_filename",
    "action_unidentified_sprite_filename",
    "action_type",
    "action_spawn_level",
    "action_spawn_probability",
    "action_spawn_requires_flag",
    "action_spawn_manual_unlock",
    "action_max_uses",
    "custom_xml_file",
    "action_mana_drain",
    "action_is_dangerous_blast",
    "action_draw_many_count",
    "action_ai_never_uses",
    "action_never_unlimited",
    "state_shuffled",
    "state_cards_drawn",
    "state_discarded_action",
    "state_destroyed_action",
    "fire_rate_wait",
    "speed_multiplier",
    "child_speed_multiplier",
    "dampening",
    "explosion_radius",
    "spread_degrees",
    "pattern_degrees",
    "screenshake",
    "recoil",
    "damage_melee_add",
    "damage_projectile_add",
    "damage_electricity_add",
    "damage_fire_add",
    "damage_explosion_add",
    "damage_ice_add",
    "damage_slice_add",
    "damage_healing_add",
    "damage_curse_add",
    "damage_drill_add",
    "damage_null_all",
    "damage_critical_chance",
    "damage_critical_multiplier",
    "explosion_damage_to_materials",
    "knockback_force",
    "reload_time",
    "lightning_count",
    "material",
    "material_amount",
    "trail_material",
    "trail_material_amount",
    "bounces",
    "gravity",
    "light",
    "blood_count_multiplier",
    "gore_particles",
    "ragdoll_fx",
    "friendly_fire",
    "physics_impulse_coeff",
    "lifetime_add",
    "sprite",
    "extra_entities",
    "game_effect_entities",
    "sound_loop_tag",
    "projectile_file",
];

#[derive(Clone, Debug, PartialEq)]
pub enum InfoValue {
    Nil,
    Bool(bool),
    Num(f64),
    Str(String),
}

/// One `RegisterGunAction` record: the modifiers (`c.*`) in force for a shot.
#[derive(Clone, Debug, PartialEq)]
pub struct ActionInfo {
    values: Vec<InfoValue>,
}

impl ActionInfo {
    pub fn get(&self, field: &str) -> Option<&InfoValue> {
        let i = ACTION_INFO_FIELDS.iter().position(|&f| f == field)?;
        self.values.get(i)
    }

    /// A numeric field (0 when absent or not a number).
    pub fn num(&self, field: &str) -> f64 {
        match self.get(field) {
            Some(InfoValue::Num(n)) => *n,
            _ => 0.0,
        }
    }

    pub fn str(&self, field: &str) -> &str {
        match self.get(field) {
            Some(InfoValue::Str(s)) => s,
            _ => "",
        }
    }

    pub fn bool(&self, field: &str) -> bool {
        matches!(self.get(field), Some(InfoValue::Bool(true)))
    }
}

/// What `gun.lua` asked the engine to do, in call order.
#[derive(Clone, Debug, PartialEq)]
pub enum GunCommand {
    BeginProjectile(String),
    EndProjectile,
    BeginTriggerTimer(i32),
    BeginTriggerHitWorld,
    BeginTriggerDeath,
    EndTrigger,
    /// The modifiers of the shot that just finished drawing.
    RegisterGunAction(Box<ActionInfo>),
    /// Applies the last registered modifiers to the projectiles of that shot.
    SetProjectileConfigs,
    RegisterGunShotEffects {
        recoil_knockback: f64,
    },
    StartReload(i32),
    ActionUsed(i32),
    ActionUsesRemainingChanged {
        inventoryitem_id: i32,
        uses_remaining: i32,
    },
    OnActionPlayed(String),
    OnNotEnoughManaForAction,
    LogAction(String),
    BaabInstruction(String),
    ReflectProjectile(String),
}

/// `ConfigGun` (`gun_generated.lua`): the wand's fixed properties.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GunConfig {
    pub actions_per_round: i32,
    pub shuffle_deck_when_empty: bool,
    pub reload_time: f64,
    pub deck_capacity: i32,
}

impl Default for GunConfig {
    fn default() -> Self {
        Self {
            actions_per_round: 1,
            shuffle_deck_when_empty: false,
            reload_time: 40.0,
            deck_capacity: 2,
        }
    }
}

/// A spell in a wand's deck order.
#[derive(Clone, Debug, PartialEq)]
pub struct DeckCard {
    pub action_id: String,
    pub inventoryitem_id: i32,
    pub uses_remaining: i32,
    pub is_identified: bool,
}

impl DeckCard {
    /// An unlimited, identified spell.
    pub fn new(action_id: &str, inventoryitem_id: i32) -> Self {
        Self {
            action_id: action_id.to_owned(),
            inventoryitem_id,
            uses_remaining: -1,
            is_identified: true,
        }
    }
}

/// A wand's casting state: `gun.lua` running inside its own Lua state.
pub struct Gun {
    runtime: Runtime,
}

fn value_of(v: &Value) -> InfoValue {
    match v {
        Value::Boolean(b) => InfoValue::Bool(*b),
        Value::Number(n) => InfoValue::Num(*n),
        Value::Integer(i) => InfoValue::Num(*i as f64),
        Value::String(s) => InfoValue::Str(s.to_string_lossy()),
        _ => InfoValue::Nil,
    }
}

fn str_arg(args: &MultiValue, i: usize) -> String {
    match args.get(i) {
        Some(Value::String(s)) => s.to_string_lossy(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Integer(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn int_arg(args: &MultiValue, i: usize) -> i32 {
    match args.get(i) {
        Some(Value::Number(n)) => sim::random::cvtt32(*n),
        Some(Value::Integer(n)) => sim::random::cvtt32(*n as f64),
        _ => 0,
    }
}

fn record(lua: &Lua, command: GunCommand) {
    if let Some(state) = lua.app_data_ref::<ScriptState>() {
        state.commands.borrow_mut().push(command);
    }
}

fn install_api(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    let simple = |name: &str, make: fn(&MultiValue) -> GunCommand| -> mlua::Result<()> {
        g.set(
            name,
            lua.create_function(move |lua, args: MultiValue| {
                record(lua, make(&args));
                Ok(())
            })?,
        )
    };
    simple("BeginProjectile", |a| {
        GunCommand::BeginProjectile(str_arg(a, 0))
    })?;
    simple("EndProjectile", |_| GunCommand::EndProjectile)?;
    simple("BeginTriggerTimer", |a| {
        GunCommand::BeginTriggerTimer(int_arg(a, 0))
    })?;
    simple("BeginTriggerHitWorld", |_| GunCommand::BeginTriggerHitWorld)?;
    simple("BeginTriggerDeath", |_| GunCommand::BeginTriggerDeath)?;
    simple("EndTrigger", |_| GunCommand::EndTrigger)?;
    simple("SetProjectileConfigs", |_| GunCommand::SetProjectileConfigs)?;
    simple("StartReload", |a| GunCommand::StartReload(int_arg(a, 0)))?;
    simple("ActionUsed", |a| GunCommand::ActionUsed(int_arg(a, 0)))?;
    simple("OnActionPlayed", |a| {
        GunCommand::OnActionPlayed(str_arg(a, 0))
    })?;
    simple("OnNotEnoughManaForAction", |_| {
        GunCommand::OnNotEnoughManaForAction
    })?;
    simple("LogAction", |a| GunCommand::LogAction(str_arg(a, 0)))?;
    simple("BaabInstruction", |a| {
        GunCommand::BaabInstruction(str_arg(a, 0))
    })?;
    simple("Reflection_RegisterProjectile", |a| {
        GunCommand::ReflectProjectile(str_arg(a, 0))
    })?;
    simple("RegisterGunShotEffects", |a| {
        GunCommand::RegisterGunShotEffects {
            recoil_knockback: match a.front() {
                Some(Value::Number(n)) => *n,
                Some(Value::Integer(n)) => *n as f64,
                _ => 0.0,
            },
        }
    })?;
    simple("RegisterGunAction", |a| {
        let mut values: Vec<InfoValue> = a.iter().map(value_of).collect();
        values.resize(ACTION_INFO_FIELDS.len(), InfoValue::Nil);
        GunCommand::RegisterGunAction(Box::new(ActionInfo { values }))
    })?;
    g.set(
        "ActionUsesRemainingChanged",
        lua.create_function(|lua, args: MultiValue| {
            record(
                lua,
                GunCommand::ActionUsesRemainingChanged {
                    inventoryitem_id: int_arg(&args, 0),
                    uses_remaining: int_arg(&args, 1),
                },
            );
            Ok(true)
        })?,
    )?;
    g.set(
        "GameGetFrameNum",
        lua.create_function(|lua, ()| {
            Ok(lua
                .app_data_ref::<ScriptState>()
                .map_or(0, |s| s.frame.get()))
        })?,
    )?;
    Ok(())
}

impl Gun {
    /// Loads `data/scripts/gun/gun.lua` from `root` (the extracted data) for a world seed.
    pub fn new(root: impl Into<std::path::PathBuf>, world_seed: i32) -> mlua::Result<Self> {
        let runtime = Runtime::new(root, world_seed)?;
        install_api(runtime.lua())?;
        runtime.dofile("data/scripts/gun/gun.lua")?;
        let gun = Self { runtime };
        gun.set_gun(&GunConfig::default())?;
        gun.set_base_state(|_| Ok(()))?;
        Ok(gun)
    }

    pub fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    fn call(&self, name: &str, args: impl mlua::IntoLuaMulti) -> mlua::Result<MultiValue> {
        let f: Function = self.runtime.lua().globals().get(name)?;
        f.call(args)
    }

    /// `GameGetFrameNum()` for the scripts (the deck shuffle is seeded with it).
    pub fn set_frame(&self, frame: i32) {
        if let Some(state) = self.runtime.lua().app_data_ref::<ScriptState>() {
            state.frame.set(frame);
        }
    }

    /// `_set_gun`: the wand's `ConfigGun` values.
    pub fn set_gun(&self, config: &GunConfig) -> mlua::Result<()> {
        let lua = self.runtime.lua();
        let data = lua.create_table()?;
        data.set("actions_per_round", config.actions_per_round)?;
        data.set("shuffle_deck_when_empty", config.shuffle_deck_when_empty)?;
        data.set("reload_time", config.reload_time)?;
        data.set("deck_capacity", config.deck_capacity)?;
        lua.globals().set("__globaldata", data)?;
        self.call("_set_gun", ())?;
        Ok(())
    }

    /// `_set_gun2`: the wand's base shot state (`ConfigGunActionInfo` defaults, then `adjust`
    /// edits the table, e.g. `fire_rate_wait` or `spread_degrees`).
    pub fn set_base_state(
        &self,
        adjust: impl FnOnce(&Table) -> mlua::Result<()>,
    ) -> mlua::Result<()> {
        let lua = self.runtime.lua();
        let data = lua.create_table()?;
        self.call("ConfigGunActionInfo_Init", data.clone())?;
        adjust(&data)?;
        lua.globals().set("__globaldata", data)?;
        self.call("_set_gun2", ())?;
        Ok(())
    }

    /// `_clear_deck` followed by `_add_card_to_deck` for each card.
    pub fn set_deck(&self, cards: &[DeckCard], use_game_log: bool) -> mlua::Result<()> {
        self.call("_clear_deck", use_game_log)?;
        for card in cards {
            self.call(
                "_add_card_to_deck",
                (
                    card.action_id.as_str(),
                    card.inventoryitem_id,
                    card.uses_remaining,
                    card.is_identified,
                ),
            )?;
        }
        Ok(())
    }

    /// `_play_permanent_card`: an always-on spell attached to the wand.
    pub fn play_permanent_card(&self, action_id: &str) -> mlua::Result<()> {
        self.call("_play_permanent_card", action_id)?;
        Ok(())
    }

    /// `_add_extra_modifier_to_shot`.
    pub fn add_extra_modifier(&self, name: &str) -> mlua::Result<()> {
        self.call("_add_extra_modifier_to_shot", name)?;
        Ok(())
    }

    /// One cast: `_start_shot`, `_draw_actions_for_shot`, and the commands it produced.
    pub fn shoot(&self, mana: f64, can_reload_at_end: bool) -> mlua::Result<Vec<GunCommand>> {
        self.take_commands();
        self.call("_start_shot", mana)?;
        self.call("_draw_actions_for_shot", can_reload_at_end)?;
        Ok(self.take_commands())
    }

    /// `_handle_reload`: discards the hand and returns the mana after the shot.
    pub fn handle_reload(&self) -> mlua::Result<(f64, Vec<GunCommand>)> {
        self.take_commands();
        let result = self.call("_handle_reload", ())?;
        let mana = match result.iter().next() {
            Some(Value::Number(n)) => *n,
            Some(Value::Integer(n)) => *n as f64,
            _ => 0.0,
        };
        Ok((mana, self.take_commands()))
    }

    /// Cards left in the deck, in order (`deck` in `gun.lua`).
    pub fn deck_ids(&self) -> mlua::Result<Vec<String>> {
        self.action_ids("deck")
    }

    pub fn discarded_ids(&self) -> mlua::Result<Vec<String>> {
        self.action_ids("discarded")
    }

    fn action_ids(&self, table: &str) -> mlua::Result<Vec<String>> {
        let t: Table = self.runtime.lua().globals().get(table)?;
        t.sequence_values::<Table>()
            .map(|a| a?.get::<String>("id"))
            .collect()
    }

    fn take_commands(&self) -> Vec<GunCommand> {
        self.runtime
            .lua()
            .app_data_ref::<ScriptState>()
            .map(|s| std::mem::take(&mut *s.commands.borrow_mut()))
            .unwrap_or_default()
    }
}
