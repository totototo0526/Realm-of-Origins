use rand::Rng;
use std::collections::{HashMap, HashSet};

use crate::hex_grid::{AsciiUnitDisplay, HexCoord, HexMap, HexPathfinder, render_ascii_map};
use crate::loader::ModDatabase;
use crate::models::{
    Action, Character, Condition, DropItem, Effect, Element, EquipSlot, FormationRole,
    MonsterArchetype, MonsterTemplate, Race, SpawnedMonster, Stats, Trigger,
};
use crate::refinery::Refinery;

/// キャラクターの計算済み実効戦闘パラメータ
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CombatStats {
    pub max_hp: u32,
    pub current_hp: i32,
    pub effective_stats: Stats,
    pub min_atk: u32,
    pub max_atk: u32,
    pub matk: u32,
    pub hit: u32,
    pub flee: u32,
    pub crit: f32, // パーセント (例: 15.0 = 15%)
    pub aspd: f32,
    pub attack_interval_ticks: u32,
    pub equip_def: u32,
    pub vit_def: u32,
    /// 武器攻撃力に対する倍率（武器研究などのパーティーオーラで上昇）
    pub weapon_atk_mult: f32,
    /// +10限界突破などによる防御力完全無視の固定追加ダメージ
    pub ignore_def_damage: u32,
    /// +10限界突破などによるオートスペル発動率の倍率（例: 2.0 = 2倍）
    pub autospell_chance_mult: f32,
}

/// 戦闘シミュレーション中の戦闘員状態
#[derive(Debug, Clone)]
pub struct Combatant {
    pub character: Character,
    pub stats: CombatStats,
    pub next_action_tick: u32,
    pub is_alive: bool,
    pub total_damage_dealt: u64,
    pub autospell_count: u32,
    pub position: HexCoord,
    pub move_ticks: u32,
    pub next_move_tick: u32,
    pub attack_range: u32,
}

/// 1回の戦闘結果
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct BattleResult {
    pub monster_name: String,
    pub monster_archetype: MonsterArchetype,
    pub total_ticks: u32,
    pub total_damage: u64,
    pub drops: Vec<DropItem>,
    pub raw_loot_value: u32,
    pub overcharge_mult: f32,
    pub final_loot_value: u32,
    pub gained_exp: u64,
    pub logs: Vec<String>,
}

/// 模擬GvG（3vs3パーティー対抗戦）の結果
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct GvgResult {
    pub winner_team: String,
    pub loser_team: String,
    pub total_ticks: u32,
    pub logs: Vec<String>,
    pub team_a_survivors: Vec<String>,
    pub team_b_survivors: Vec<String>,
}

/// 空間戦場での戦闘結果（チョークポイント防衛戦・空間ハラスメント）
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SpatialBattleResult {
    pub map_name: String,
    pub total_ticks: u32,
    pub total_damage: u64,
    pub defeated_monsters: usize,
    pub drops: Vec<DropItem>,
    pub raw_loot_value: u32,
    pub overcharge_mult: f32,
    pub final_loot_value: u32,
    pub gained_exp: u64,
    pub logs: Vec<String>,
    pub ascii_snapshots: Vec<String>,
    pub is_victory: bool,
}

/// 空間戦場でのGvG対抗戦結果（陣形激突・A*移動・AoE・ノックバック）
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SpatialGvgResult {
    pub map_name: String,
    pub winner_team: String,
    pub loser_team: String,
    pub total_ticks: u32,
    pub logs: Vec<String>,
    pub ascii_snapshots: Vec<String>,
    pub team_a_survivors: Vec<String>,
    pub team_b_survivors: Vec<String>,
}

/// 10連戦などのシミュレーション全体結果
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct SimulationSummary {
    pub party_name: String,
    pub battles: Vec<BattleResult>,
    pub total_ticks: u32,
    pub average_ticks: f32,
    pub total_damage: u64,
    pub total_raw_loot: u32,
    pub total_final_loot: u32,
    pub total_autospells: u32,
    pub total_exp_gained: u64,
}

pub struct SimulationEngine<'a> {
    pub db: &'a ModDatabase,
}

impl<'a> SimulationEngine<'a> {
    pub fn new(db: &'a ModDatabase) -> Self {
        Self { db }
    }

    /// モンスターテンプレートから個体差（Archetype）を適用した実体を生成する
    pub fn spawn_monster(&self, template: &MonsterTemplate, archetype: MonsterArchetype) -> SpawnedMonster {
        let mut stats = template.base_stats;
        let mut hp = template.base_hp;

        // 個体差テンプレートによるステータス自動補正
        match archetype {
            MonsterArchetype::Standard => {
                // 標準型: 基礎ステータスをそのまま使用
            }
            MonsterArchetype::Swift => {
                // 迅速型: AGI/DEXが大幅上昇。FLEEとASPDが高く、攻撃を回避しやすい
                stats.agi = ((stats.agi as f32) * 1.5 + (template.level as f32) * 0.4) as u16;
                stats.dex = ((stats.dex as f32) * 1.3) as u16;
                stats.vit = ((stats.vit as f32) * 0.8) as u16;
            }
            MonsterArchetype::Heavy => {
                // 重装型: VIT/STR特化。HPと防御力が高く、物理攻撃が重い
                stats.vit = ((stats.vit as f32) * 1.5 + (template.level as f32) * 0.5) as u16;
                stats.str_stat = ((stats.str_stat as f32) * 1.35) as u16;
                stats.agi = ((stats.agi as f32) * 0.7) as u16;
                hp = ((hp as f32) * 1.45) as u32;
            }
            MonsterArchetype::Lucky => {
                // 幸運型: LUKが大幅上昇。完全回避やクリティカル耐性に優れる
                stats.luk = ((stats.luk as f32) * 2.5 + 25.0) as u16;
            }
        }

        SpawnedMonster {
            template_id: template.id.clone(),
            name: format!("{}({})", template.name, archetype.name_ja()),
            archetype,
            level: template.level,
            race: template.race,
            element: template.element,
            size: template.size,
            stats,
            current_hp: hp as i32,
            max_hp: hp,
            attack_range: template.attack_range,
            drops: template.drops.clone(),
        }
    }

    /// パーティー全体のオーラ効果を計算
    /// 特に `PartyWeaponResearch` (非戦闘職が味方の武器攻撃力を底上げするシナジー)
    pub fn calculate_party_weapon_multiplier(&self, party: &[Character]) -> f32 {
        let mut max_multiplier = 1.0f32;

        for member in party {
            for skill in &member.learned_skills {
                for modifier in &skill.modifiers {
                    if modifier.trigger == Trigger::PartyAura {
                        if let Effect::PartyWeaponResearch {
                            base_bonus,
                            ref scaling_stats,
                        } = modifier.effect
                        {
                            let sum_stats: u32 = scaling_stats
                                .iter()
                                .map(|st| member.base_stats.get(*st) as u32)
                                .sum();

                            let mult = 1.0 + base_bonus * (sum_stats as f32 / 100.0);
                            if mult > max_multiplier {
                                max_multiplier = mult;
                            }
                        }
                    }
                }
            }
        }

        max_multiplier
    }

    /// 戦闘終了時の換金ボーナス（Overcharge）倍率を計算
    pub fn calculate_overcharge_multiplier(&self, party: &[Character]) -> f32 {
        let mut max_multiplier = 1.0f32;

        for member in party {
            for skill in &member.learned_skills {
                for modifier in &skill.modifiers {
                    if modifier.trigger == Trigger::OnBattleEnd {
                        if let Effect::OverchargeBonus {
                            base_bonus,
                            scaling_stat,
                        } = modifier.effect
                        {
                            let stat_val = member.base_stats.get(scaling_stat) as f32;
                            let mult = 1.0 + base_bonus * (stat_val / 100.0);
                            if mult > max_multiplier {
                                max_multiplier = mult;
                            }
                        }
                    }
                }
            }
        }

        max_multiplier
    }

    /// キャラクターの実効戦闘ステータスを計算（ROクラシック仕様準拠 ＋ 過剰精錬限界突破反映）
    pub fn calculate_combat_stats(
        &self,
        character: &Character,
        weapon_mult: f32,
    ) -> CombatStats {
        let mut effective_stats = character.base_stats;

        // 1. パッシブModifierによるステータス補正の集計
        if let Some(job) = self.db.jobs.get(&character.job_id) {
            for modifier in &job.modifiers {
                if modifier.trigger == Trigger::Passive {
                    if let Effect::StatBonus { stat, flat, percent } = modifier.effect {
                        effective_stats.add(stat, flat);
                        if percent > 0.0 {
                            let val = effective_stats.get(stat) as f32;
                            let bonus = (val * percent).round() as i32;
                            effective_stats.add(stat, bonus);
                        }
                    }
                }
            }
        }

        // 装備・カードパッシブ
        for equip in &character.equipments {
            for modifier in &equip.modifiers {
                if modifier.trigger == Trigger::Passive {
                    if let Effect::StatBonus { stat, flat, percent } = modifier.effect {
                        effective_stats.add(stat, flat);
                        if percent > 0.0 {
                            let val = effective_stats.get(stat) as f32;
                            let bonus = (val * percent).round() as i32;
                            effective_stats.add(stat, bonus);
                        }
                    }
                }
            }
            for card in &equip.cards {
                for modifier in &card.modifiers {
                    if modifier.trigger == Trigger::Passive {
                        if let Effect::StatBonus { stat, flat, percent } = modifier.effect {
                            effective_stats.add(stat, flat);
                            if percent > 0.0 {
                                let val = effective_stats.get(stat) as f32;
                                let bonus = (val * percent).round() as i32;
                                effective_stats.add(stat, bonus);
                            }
                        }
                    }
                }
            }
        }

        // スキルパッシブ
        for skill in &character.learned_skills {
            for modifier in &skill.modifiers {
                if modifier.trigger == Trigger::Passive {
                    if let Effect::StatBonus { stat, flat, percent } = modifier.effect {
                        effective_stats.add(stat, flat);
                        if percent > 0.0 {
                            let val = effective_stats.get(stat) as f32;
                            let bonus = (val * percent).round() as i32;
                            effective_stats.add(stat, bonus);
                        }
                    }
                }
            }
        }

        // 2. HP計算（レベルとVITに応じた自然なHP成長）
        let (base_hp, hp_factor, base_aspd) = if let Some(job) = self.db.jobs.get(&character.job_id) {
            (job.base_hp, job.hp_factor, job.base_aspd)
        } else {
            (1000, 1.0, 150.0)
        };
        let lvl_factor = 1.0 + (character.level as f32 * 0.035);
        let vit_hp_bonus = 1.0 + (effective_stats.vit as f32 * 0.01);
        let max_hp = ((base_hp as f32) * lvl_factor * vit_hp_bonus * hp_factor).round() as u32;

        // 3. ATK計算（STR二乗ボーナス + 精錬ボーナス）
        let str_val = effective_stats.str_stat as u32;
        let dex_val = effective_stats.dex as u32;
        let str_bonus = (str_val / 10).pow(2);
        let base_atk = str_val + str_bonus + (dex_val / 5);

        // 武器ATK（武器研究倍率＋精錬ボーナス）
        let mut total_weapon_atk = 0u32;
        let mut total_equip_def = 0u32;
        let mut refine_crit_bonus = 0.0f32;
        let mut refine_aspd_bonus = 0.0f32;
        let mut total_ignore_def_dmg = 0u32;
        let mut max_autospell_mult = 1.0f32;

        for equip in &character.equipments {
            // 精錬1ごとに武器ATK+5
            let refine_atk = equip.refine as u32 * 5;
            total_weapon_atk += equip.weapon_atk + refine_atk;
            total_equip_def += equip.refine as u32 * 2;

            if equip.slot == EquipSlot::Weapon {
                let (aspd_b, crit_b, ign_dmg, as_mult) = Refinery::get_breakthrough_bonus(equip.refine);
                refine_aspd_bonus += aspd_b;
                refine_crit_bonus += crit_b;
                total_ignore_def_dmg += ign_dmg;
                if as_mult > max_autospell_mult {
                    max_autospell_mult = as_mult;
                }
            }
        }
        let effective_weapon_atk = ((total_weapon_atk as f32) * weapon_mult).round() as u32;

        let min_atk = base_atk + (effective_weapon_atk * dex_val.min(100) / 100);
        let max_atk = base_atk + effective_weapon_atk;

        // 4. MATK
        let int_val = effective_stats.int as u32;
        let matk = int_val + (int_val / 7).pow(2);

        // 5. HIT & FLEE & CRIT
        let hit = character.level + dex_val;
        let flee = character.level + effective_stats.agi as u32;
        let crit = 1.0 + (effective_stats.luk as f32 * 0.3) + refine_crit_bonus;

        // 6. ASPD
        let aspd = base_aspd
            + (effective_stats.agi as f32 * 0.22)
            + (effective_stats.dex as f32 * 0.08)
            + refine_aspd_bonus;
        let interval = (((200.0 - aspd.min(195.0)) / 2.0).round().max(4.0)) as u32;

        // 7. 防御値
        let vit_def = (effective_stats.vit as u32 * 8) / 10;

        CombatStats {
            max_hp,
            current_hp: max_hp as i32,
            effective_stats,
            min_atk,
            max_atk: max_atk.max(min_atk),
            matk,
            hit,
            flee,
            crit,
            aspd,
            attack_interval_ticks: interval,
            equip_def: total_equip_def,
            vit_def,
            weapon_atk_mult: weapon_mult,
            ignore_def_damage: total_ignore_def_dmg,
            autospell_chance_mult: max_autospell_mult,
        }
    }

    /// 種族特攻カード・ボーナスの集計
    pub fn calculate_race_damage_multiplier(&self, character: &Character, target_race: Race) -> f32 {
        let mut race_bonuses: HashMap<Race, f32> = HashMap::new();

        for equip in &character.equipments {
            for modifier in &equip.modifiers {
                if modifier.trigger == Trigger::Passive {
                    if let Effect::RaceDamageBonus { race, percent } = modifier.effect {
                        *race_bonuses.entry(race).or_insert(0.0) += percent;
                    }
                }
            }
            for card in &equip.cards {
                for modifier in &card.modifiers {
                    if modifier.trigger == Trigger::Passive {
                        if let Effect::RaceDamageBonus { race, percent } = modifier.effect {
                            *race_bonuses.entry(race).or_insert(0.0) += percent;
                        }
                    }
                }
            }
        }

        let bonus = race_bonuses.get(&target_race).copied().unwrap_or(0.0);
        1.0 + bonus
    }

    /// キャラクターの装備武器から攻撃射程を取得（未装備時は1）
    pub fn get_character_attack_range(ch: &Character) -> u32 {
        ch.equipments
            .iter()
            .find(|e| e.slot == EquipSlot::Weapon)
            .map(|e| e.attack_range)
            .unwrap_or(1)
    }

    /// キャラクターのAGIに応じた移動間隔Tick（AGIが高いほど速く移動可能）
    pub fn get_character_move_ticks(ch: &Character) -> u32 {
        ((22.0 - (ch.base_stats.agi as f32 * 0.12)).max(6.0).round()) as u32
    }

    /// 1体のモンスターに対するパーティーの戦闘シミュレーションを実行する
    pub fn simulate_battle(
        &self,
        party: &[Character],
        monster: &mut SpawnedMonster,
    ) -> BattleResult {
        let mut rng = rand::thread_rng();
        let weapon_mult = self.calculate_party_weapon_multiplier(party);
        let overcharge_mult = self.calculate_overcharge_multiplier(party);

        // 戦闘員リストの構築
        let mut combatants: Vec<Combatant> = party
            .iter()
            .map(|ch| {
                let stats = self.calculate_combat_stats(ch, weapon_mult);
                let attack_range = Self::get_character_attack_range(ch);
                let move_ticks = Self::get_character_move_ticks(ch);
                Combatant {
                    character: ch.clone(),
                    stats,
                    next_action_tick: 0,
                    is_alive: true,
                    total_damage_dealt: 0,
                    autospell_count: 0,
                    position: HexCoord::new(0, 0),
                    move_ticks,
                    next_move_tick: 0,
                    attack_range,
                }
            })
            .collect();

        // モンスターの戦闘パラメータ
        let monster_hit = monster.level + monster.stats.dex as u32;
        let monster_flee = monster.level + monster.stats.agi as u32;
        let monster_str = monster.stats.str_stat as u32;
        let monster_atk_min = monster_str + (monster.stats.dex as u32 / 4);
        let monster_atk_max = monster_str * 2 + (monster.stats.str_stat as u32 / 10).pow(2);
        let monster_aspd = 140.0 + (monster.stats.agi as f32 * 0.2);
        let monster_interval = (((200.0 - monster_aspd.min(190.0)) / 2.0).round().max(6.0)) as u32;
        let mut monster_next_action_tick = 0u32;

        let mut current_tick = 0u32;
        let mut total_damage_to_monster = 0u64;
        let mut logs = Vec::new();

        logs.push(format!(
            "⚔️  戦闘開始: {} (HP: {}, 種族: {:?}, 属性: {:?})",
            monster.name, monster.max_hp, monster.race, monster.element
        ));
        logs.push(format!(
            "   個体特徴: {}",
            monster.archetype.description()
        ));
        if weapon_mult > 1.0 {
            logs.push(format!(
                "   ✨ パーティー武器研究オーラ発動中: 武器ATK +{:.1}%",
                (weapon_mult - 1.0) * 100.0
            ));
        }

        // Tickベースの戦闘ループ
        while monster.current_hp > 0 && combatants.iter().any(|c| c.is_alive) && current_tick < 10000 {
            // 1. プレイヤー側の行動判定
            for c_idx in 0..combatants.len() {
                if !combatants[c_idx].is_alive {
                    continue;
                }
                if combatants[c_idx].next_action_tick <= current_tick {
                    let character = combatants[c_idx].character.clone();
                    let c_stats = combatants[c_idx].stats.clone();

                    // ガンビットルールの評価（優先度順）
                    let mut chosen_action = Action::NormalAttack;
                    let mut matched_rule_id = String::new();
                    let mut sorted_rules = character.gambit_rules.clone();
                    sorted_rules.sort_by_key(|r| r.priority);

                    for rule in &sorted_rules {
                        let matched = match &rule.condition {
                            Condition::TargetArchetypeIs(arch) => monster.archetype == *arch,
                            Condition::TargetRaceIs(race) => monster.race == *race,
                            Condition::SelfHpBelow(ratio) => {
                                (c_stats.current_hp as f32 / c_stats.max_hp as f32) < *ratio
                            }
                            Condition::TargetVitAbove(threshold) => {
                                (monster.stats.vit as u32) >= *threshold
                            }
                            Condition::TargetDistanceAtLeast(dist) => 1 >= *dist,
                            Condition::TargetDistanceAtMost(dist) => 1 <= *dist,
                            Condition::EnemiesInRadius { min_count, .. } => 1 >= *min_count,
                            Condition::Always => true,
                        };
                        if matched {
                            chosen_action = rule.action.clone();
                            matched_rule_id = rule.id.clone();
                            break;
                        }
                    }

                    // アクションの実行
                    match chosen_action {
                        Action::UseActiveSkill(ref skill_id) => {
                            // ガンビットによるアクティブスキル使用
                            let skill_opt = self.db.skills.get(skill_id);
                            let is_sure_hit = skill_opt.map(|s| s.is_sure_hit).unwrap_or(false);
                            let phys_mult = skill_opt.and_then(|s| s.physical_multiplier).unwrap_or(1.0);
                            let skill_name = skill_opt.map(|s| s.name.as_str()).unwrap_or(skill_id);

                            let base_damage = rng.gen_range(c_stats.min_atk..=c_stats.max_atk) as f32 * phys_mult;
                            let race_mult = self.calculate_race_damage_multiplier(&character, monster.race);
                            let monster_def = monster.stats.vit as f32 * 0.4;
                            let reduced = (base_damage - monster_def).max(1.0);
                            // 防御無視固定ダメージ（+10過剰精錬ボーナス）を加算
                            let final_damage = (reduced * race_mult).round() as u32 + c_stats.ignore_def_damage;

                            monster.current_hp -= final_damage as i32;
                            combatants[c_idx].total_damage_dealt += final_damage as u64;
                            total_damage_to_monster += final_damage as u64;

                            let sure_tag = if is_sure_hit { " [必中強撃・FLEE無効]" } else { "" };
                            let ign_tag = if c_stats.ignore_def_damage > 0 {
                                format!(" (限界突破防御無視+{})", c_stats.ignore_def_damage)
                            } else {
                                "".to_string()
                            };

                            logs.push(format!(
                                "   ⚡ [Tick {:>4}] {} のガンビット発動 [{}] -> スキル【{}】で攻撃！ {} に {} ダメージ!{}{}",
                                current_tick, character.name, matched_rule_id, skill_name, monster.name, final_damage, sure_tag, ign_tag
                            ));

                            // オートスペル発動判定（スキル攻撃時にも連動判定）
                            self.process_autospell_on_hit(
                                &mut combatants[c_idx],
                                monster,
                                current_tick,
                                &mut logs,
                            );
                        }
                        Action::SwitchWeapon(_) => {
                            // 武器持ち替え
                            logs.push(format!(
                                "   🔄 [Tick {:>4}] {} は武器を持ち替えた！",
                                current_tick, character.name
                            ));
                        }
                        Action::NormalAttack => {
                            // 通常攻撃
                            let hit_rate = (80 + c_stats.hit as i32 - monster_flee as i32).clamp(5, 95);
                            let hit_roll: i32 = rng.gen_range(1..=100);

                            let crit_roll: f32 = rng.gen_range(0.0..100.0);
                            let is_crit = crit_roll < (c_stats.crit - (monster.stats.luk as f32 * 0.15));

                            if is_crit || hit_roll <= hit_rate {
                                let base_damage = rng.gen_range(c_stats.min_atk..=c_stats.max_atk) as f32;
                                let race_mult = self.calculate_race_damage_multiplier(&character, monster.race);

                                let final_damage = if is_crit {
                                    (base_damage * race_mult * 1.4).round() as u32 + c_stats.ignore_def_damage
                                } else {
                                    let monster_def = monster.stats.vit as f32 * 0.5;
                                    let reduced = (base_damage - monster_def).max(1.0);
                                    (reduced * race_mult).round() as u32 + c_stats.ignore_def_damage
                                };

                                monster.current_hp -= final_damage as i32;
                                combatants[c_idx].total_damage_dealt += final_damage as u64;
                                total_damage_to_monster += final_damage as u64;

                                let crit_tag = if is_crit { " [CRITICAL! 必中防御無視]" } else { "" };
                                let race_tag = if race_mult > 1.0 {
                                    format!(" (特化特効x{:.2})", race_mult)
                                } else {
                                    "".to_string()
                                };
                                let ign_tag = if c_stats.ignore_def_damage > 0 {
                                    format!(" (限界突破+{})", c_stats.ignore_def_damage)
                                } else {
                                    "".to_string()
                                };

                                if is_crit || final_damage > 150 {
                                    logs.push(format!(
                                        "   [Tick {:>4}] {} の攻撃 -> {} に {} ダメージ!{}{}{}",
                                        current_tick, character.name, monster.name, final_damage, race_tag, crit_tag, ign_tag
                                    ));
                                }

                                self.process_autospell_on_hit(
                                    &mut combatants[c_idx],
                                    monster,
                                    current_tick,
                                    &mut logs,
                                );
                            } else {
                                // MISS
                                if rng.gen_bool(0.20) {
                                    logs.push(format!(
                                        "   [Tick {:>4}] {} の攻撃 -> {} に回避された! (MISS)",
                                        current_tick, character.name, monster.name
                                    ));
                                }
                            }
                        }
                    }

                    if monster.current_hp <= 0 {
                        break;
                    }

                    // 次の攻撃Tick設定
                    combatants[c_idx].next_action_tick = current_tick + c_stats.attack_interval_ticks;
                }
            }

            if monster.current_hp <= 0 {
                break;
            }

            // 2. モンスター側の行動判定
            if monster_next_action_tick <= current_tick {
                let mut target_idx = None;
                let mut max_vit = 0;
                for (idx, c) in combatants.iter().enumerate() {
                    if c.is_alive && c.stats.effective_stats.vit >= max_vit {
                        max_vit = c.stats.effective_stats.vit;
                        target_idx = Some(idx);
                    }
                }

                if let Some(t_idx) = target_idx {
                    let target_stats = &combatants[t_idx].stats;
                    let m_hit_rate = (80 + monster_hit as i32 - target_stats.flee as i32).clamp(5, 95);
                    let m_roll: i32 = rng.gen_range(1..=100);

                    if m_roll <= m_hit_rate {
                        let m_raw_damage = rng.gen_range(monster_atk_min..=monster_atk_max);
                        let player_def = target_stats.vit_def + target_stats.equip_def;
                        let m_damage = m_raw_damage.saturating_sub(player_def).max(1);

                        combatants[t_idx].stats.current_hp -= m_damage as i32;

                        if combatants[t_idx].stats.current_hp <= 0 {
                            combatants[t_idx].is_alive = false;
                            logs.push(format!(
                                "   💀 [Tick {:>4}] {} は倒れた！",
                                current_tick, combatants[t_idx].character.name
                            ));
                        }
                    }
                }

                monster_next_action_tick = current_tick + monster_interval;
            }

            current_tick += 1;
        }

        // 3. ドロップアイテム抽選とOvercharge換金計算、および経験値
        let mut drops = Vec::new();
        let mut raw_loot_value = 0u32;
        let mut gained_exp = 0u64;

        if monster.current_hp <= 0 {
            logs.push(format!(
                "🏆 [Tick {:>4}] {} を討伐した！（総ダメージ: {}）",
                current_tick, monster.name, total_damage_to_monster
            ));

            let drop_rate_mult = if monster.archetype == MonsterArchetype::Lucky { 1.4 } else { 1.0 };

            for item in &monster.drops {
                let chance = (item.drop_rate * drop_rate_mult).min(1.0);
                if rng.gen::<f32>() < chance {
                    drops.push(item.clone());
                    raw_loot_value += item.base_value;
                    logs.push(format!(
                        "   🎁 戦利品獲得: {} (基礎価値: {} Zeny)",
                        item.name, item.base_value
                    ));
                }
            }

            // 経験値計算（個体差ボーナス反映）
            let base_exp = self.db.monsters.get(&monster.template_id).map(|m| m.base_exp).unwrap_or(400) as f32;
            let exp_mult = match monster.archetype {
                MonsterArchetype::Lucky => 1.5,
                MonsterArchetype::Heavy => 1.3,
                MonsterArchetype::Swift => 1.2,
                MonsterArchetype::Standard => 1.0,
            };
            gained_exp = (base_exp * exp_mult).round() as u64;
            logs.push(format!("   ⭐ 獲得経験値: {} EXP", gained_exp));
        }

        let final_loot_value = ((raw_loot_value as f32) * overcharge_mult).round() as u32;

        BattleResult {
            monster_name: monster.name.clone(),
            monster_archetype: monster.archetype,
            total_ticks: current_tick,
            total_damage: total_damage_to_monster,
            drops,
            raw_loot_value,
            overcharge_mult,
            final_loot_value,
            gained_exp,
            logs,
        }
    }

    /// 通常攻撃命中時のオートスペル（AutoSpell）判定と魔法発動
    fn process_autospell_on_hit(
        &self,
        combatant: &mut Combatant,
        monster: &mut SpawnedMonster,
        current_tick: u32,
        logs: &mut Vec<String>,
    ) {
        let mut rng = rand::thread_rng();
        let as_mult = combatant.stats.autospell_chance_mult;

        for equip in &combatant.character.equipments {
            for card in &equip.cards {
                for modifier in &card.modifiers {
                    if let Trigger::OnNormalAttackHit { chance } = modifier.trigger {
                        let final_chance = (chance * as_mult).min(1.0);
                        if rng.gen::<f32>() < final_chance {
                            if let Effect::AutoSpell { ref skill_id, level } = modifier.effect {
                                if let Some(skill) = self.db.skills.get(skill_id) {
                                    let mult = skill.magic_multiplier.unwrap_or(level as f32);
                                    let element_mult = if skill_id == "fire_bolt" && monster.element == Element::Earth {
                                        1.5f32
                                    } else {
                                        1.0f32
                                    };

                                    let magic_dmg = ((combatant.stats.matk as f32) * mult * element_mult).round() as u32;
                                    monster.current_hp -= magic_dmg as i32;
                                    combatant.total_damage_dealt += magic_dmg as u64;
                                    combatant.autospell_count += 1;

                                    let boost_tag = if as_mult > 1.0 { " [🌟限界突破発動率2倍]" } else { "" };

                                    logs.push(format!(
                                        "   🔮★ [Tick {:>4}] {} の {}Lv{} がオートスペル発動！ {} に {} の追加魔法ダメージ！(弱点x{:.1}){}",
                                        current_tick, combatant.character.name, skill.name, level, monster.name, magic_dmg, element_mult, boost_tag
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// 10連戦シミュレーションを実行してサマリーをまとめる
    #[allow(dead_code)]
    pub fn run_scenario(
        &self,
        party_name: &str,
        party: &[Character],
        monsters: &[SpawnedMonster],
    ) -> SimulationSummary {
        let mut battles = Vec::new();
        let mut total_ticks = 0u32;
        let mut total_damage = 0u64;
        let mut total_raw_loot = 0u32;
        let mut total_final_loot = 0u32;
        let mut total_autospells = 0u32;
        let mut total_exp_gained = 0u64;

        for m_orig in monsters {
            let mut monster = m_orig.clone();
            let result = self.simulate_battle(party, &mut monster);
            total_ticks += result.total_ticks;
            total_damage += result.total_damage;
            total_raw_loot += result.raw_loot_value;
            total_final_loot += result.final_loot_value;
            total_exp_gained += result.gained_exp;
            for log in &result.logs {
                if log.contains("オートスペル発動") {
                    total_autospells += 1;
                }
            }
            battles.push(result);
        }

        let average_ticks = if !battles.is_empty() {
            total_ticks as f32 / battles.len() as f32
        } else {
            0.0
        };

        SimulationSummary {
            party_name: party_name.to_string(),
            battles,
            total_ticks,
            average_ticks,
            total_damage,
            total_raw_loot,
            total_final_loot,
            total_autospells,
            total_exp_gained,
        }
    }

    /// 3vs3 パーティー対抗模擬戦（GvGオートチェス形式）
    /// プレイヤー現主力 vs かつて自分が育てて放出した古参キャラたちの激突！
    pub fn simulate_gvg_battle(
        &self,
        team_a: &[Character],
        team_b: &[Character],
        name_a: &str,
        name_b: &str,
        town_bonus_a: Option<crate::guild::TownPassiveBonus>,
    ) -> GvgResult {
        let mut rng = rand::thread_rng();
        let town_atk_bonus = town_bonus_a.as_ref().map(|b| b.extra_weapon_atk_percent).unwrap_or(0.0);
        let weapon_mult_a = self.calculate_party_weapon_multiplier(team_a) + town_atk_bonus;
        let weapon_mult_b = self.calculate_party_weapon_multiplier(team_b);

        let mut combatants_a: Vec<Combatant> = team_a
            .iter()
            .map(|ch| {
                let stats = self.calculate_combat_stats(ch, weapon_mult_a);
                let attack_range = Self::get_character_attack_range(ch);
                let move_ticks = Self::get_character_move_ticks(ch);
                Combatant {
                    character: ch.clone(),
                    stats,
                    next_action_tick: 0,
                    is_alive: true,
                    total_damage_dealt: 0,
                    autospell_count: 0,
                    position: HexCoord::new(0, 0),
                    move_ticks,
                    next_move_tick: 0,
                    attack_range,
                }
            })
            .collect();

        let mut combatants_b: Vec<Combatant> = team_b
            .iter()
            .map(|ch| {
                let stats = self.calculate_combat_stats(ch, weapon_mult_b);
                let attack_range = Self::get_character_attack_range(ch);
                let move_ticks = Self::get_character_move_ticks(ch);
                Combatant {
                    character: ch.clone(),
                    stats,
                    next_action_tick: 0,
                    is_alive: true,
                    total_damage_dealt: 0,
                    autospell_count: 0,
                    position: HexCoord::new(0, 0),
                    move_ticks,
                    next_move_tick: 0,
                    attack_range,
                }
            })
            .collect();

        let mut logs = Vec::new();
        logs.push(format!("🏆 【模擬GvG開戦】『{}』 VS 『{}』", name_a, name_b));
        logs.push(format!(
            "   陣形オーラ: 『{}』武器ATK+{}% | 『{}』武器ATK+{}%",
            name_a, ((weapon_mult_a - 1.0) * 100.0).round(),
            name_b, ((weapon_mult_b - 1.0) * 100.0).round()
        ));
        if let Some(tb) = &town_bonus_a {
            logs.push(format!(
                "   🏛️ [街NPC古参バフ] 初代鍛冶屋の鍛錬支援: 『{}』の武器ATK +{:.1}% 加算！",
                name_a, tb.extra_weapon_atk_percent * 100.0
            ));
        }

        let mut current_tick = 0u32;

        while combatants_a.iter().any(|c| c.is_alive)
            && combatants_b.iter().any(|c| c.is_alive)
            && current_tick < 15000
        {
            // チームAの行動
            Self::process_gvg_team_turn(
                &self,
                &mut combatants_a,
                &mut combatants_b,
                name_a,
                name_b,
                current_tick,
                &mut rng,
                &mut logs,
            );

            // チームBの行動
            Self::process_gvg_team_turn(
                &self,
                &mut combatants_b,
                &mut combatants_a,
                name_b,
                name_a,
                current_tick,
                &mut rng,
                &mut logs,
            );

            current_tick += 1;
        }

        let a_alive = combatants_a.iter().any(|c| c.is_alive);
        let b_alive = combatants_b.iter().any(|c| c.is_alive);

        let (winner, loser) = if a_alive && !b_alive {
            (name_a.to_string(), name_b.to_string())
        } else if b_alive && !a_alive {
            (name_b.to_string(), name_a.to_string())
        } else {
            ("引き分け (Time Over)".to_string(), "".to_string())
        };

        let team_a_survivors: Vec<String> = combatants_a
            .iter()
            .filter(|c| c.is_alive)
            .map(|c| format!("{} (HP: {}/{})", c.character.name, c.stats.current_hp, c.stats.max_hp))
            .collect();

        let team_b_survivors: Vec<String> = combatants_b
            .iter()
            .filter(|c| c.is_alive)
            .map(|c| format!("{} (HP: {}/{})", c.character.name, c.stats.current_hp, c.stats.max_hp))
            .collect();

        logs.push(format!(
            "\n🏁 【GvG決着 (Tick {})】勝者: 『{}』！",
            current_tick, winner
        ));

        GvgResult {
            winner_team: winner,
            loser_team: loser,
            total_ticks: current_tick,
            logs,
            team_a_survivors,
            team_b_survivors,
        }
    }

    /// GvGでの1チーム分の攻撃ターン処理
    fn process_gvg_team_turn(
        &self,
        attackers: &mut [Combatant],
        defenders: &mut [Combatant],
        _atk_team_name: &str,
        def_team_name: &str,
        current_tick: u32,
        rng: &mut rand::rngs::ThreadRng,
        logs: &mut Vec<String>,
    ) {
        for a_idx in 0..attackers.len() {
            if !attackers[a_idx].is_alive {
                continue;
            }
            if attackers[a_idx].next_action_tick <= current_tick {
                let attacker_char = attackers[a_idx].character.clone();
                let a_stats = attackers[a_idx].stats.clone();

                // ターゲット選定: 生存している敵の中で最もVIT/HPが高い前衛壁役を優先
                let mut target_idx = None;
                let mut max_vit = 0;
                for (d_idx, d) in defenders.iter().enumerate() {
                    if d.is_alive && d.stats.effective_stats.vit >= max_vit {
                        max_vit = d.stats.effective_stats.vit;
                        target_idx = Some(d_idx);
                    }
                }

                if let Some(t_idx) = target_idx {
                    let d_stats = defenders[t_idx].stats.clone();
                    let target_name = defenders[t_idx].character.name.clone();

                    // ガンビットルールの評価
                    let mut chosen_skill_id: Option<String> = None;
                    for rule in &attacker_char.gambit_rules {
                        let matches = match &rule.condition {
                            Condition::SelfHpBelow(ratio) => {
                                (a_stats.current_hp as f32 / a_stats.max_hp as f32) < *ratio
                            }
                            Condition::TargetVitAbove(threshold) => {
                                (d_stats.effective_stats.vit as u32) >= *threshold
                            }
                            Condition::Always => true,
                            _ => false,
                        };
                        if matches {
                            if let Action::UseActiveSkill(ref s) = rule.action {
                                chosen_skill_id = Some(s.clone());
                                break;
                            }
                        }
                    }

                    // 回復スキルの処理（サンクチュアリなど）
                    if let Some(ref sid) = chosen_skill_id {
                        if let Some(sk) = self.db.skills.get(sid) {
                            if let Some(heal) = sk.heal_amount {
                                let mut lowest_idx = a_idx;
                                let mut lowest_ratio = attackers[a_idx].stats.current_hp as f32 / attackers[a_idx].stats.max_hp as f32;
                                for (idx, ally) in attackers.iter().enumerate() {
                                    if ally.is_alive {
                                        let r = ally.stats.current_hp as f32 / ally.stats.max_hp as f32;
                                        if r < lowest_ratio {
                                            lowest_ratio = r;
                                            lowest_idx = idx;
                                        }
                                    }
                                }
                                let target_ally_name = attackers[lowest_idx].character.name.clone();
                                attackers[lowest_idx].stats.current_hp = (attackers[lowest_idx].stats.current_hp + heal as i32).min(attackers[lowest_idx].stats.max_hp as i32);
                                logs.push(format!(
                                    "   💚 [Tick {:>4}] {} はスキル【{}】を詠唱！ 『{}』 のHPを {} 回復！ (HP: {}/{})",
                                    current_tick, attacker_char.name, sk.name, target_ally_name, heal,
                                    attackers[lowest_idx].stats.current_hp, attackers[lowest_idx].stats.max_hp
                                ));
                                attackers[a_idx].next_action_tick = current_tick + a_stats.attack_interval_ticks;
                                continue;
                            }
                        }
                    }

                    let (is_sure, phys_mult, is_piercing, skill_name_opt) = if let Some(ref sid) = chosen_skill_id {
                        let sk = self.db.skills.get(sid);
                        (
                            sk.map(|s| s.is_sure_hit).unwrap_or(false),
                            sk.and_then(|s| s.physical_multiplier).unwrap_or(1.0),
                            sk.map(|s| s.is_piercing_def).unwrap_or(false),
                            sk.map(|s| s.name.clone()),
                        )
                    } else {
                        (false, 1.0, false, None)
                    };

                    let hit_rate = (80 + a_stats.hit as i32 - d_stats.flee as i32).clamp(10, 95);
                    let hit_roll: i32 = rng.gen_range(1..=100);
                    let crit_roll: f32 = rng.gen_range(0.0..100.0);
                    let is_crit = crit_roll < (a_stats.crit - (d_stats.effective_stats.luk as f32 * 0.15));

                    if is_sure || is_crit || hit_roll <= hit_rate {
                        let base_dmg = rng.gen_range(a_stats.min_atk..=a_stats.max_atk) as f32 * phys_mult;
                        // PvPではプレイヤーはDemihuman（人間）扱い
                        let race_mult = self.calculate_race_damage_multiplier(&attacker_char, Race::Demihuman);

                        let def_val = (d_stats.vit_def + d_stats.equip_def) as f32 * 0.5;
                        let raw_dmg = if is_crit {
                            base_dmg * race_mult * 1.4
                        } else if is_piercing {
                            // 錐効果: 敵の防御力・装甲（DEF/VIT）が高いほど大ダメージ！
                            let pierce_bonus = (def_val * 4.2).max(60.0);
                            (base_dmg + pierce_bonus) * race_mult
                        } else {
                            (base_dmg - def_val).max(1.0) * race_mult
                        };

                        let final_dmg = raw_dmg.round() as u32 + a_stats.ignore_def_damage;
                        defenders[t_idx].stats.current_hp -= final_dmg as i32;
                        attackers[a_idx].total_damage_dealt += final_dmg as u64;

                        let act_desc = if is_piercing {
                            format!("スキル【{}】[装甲貫通・錐効果！]", skill_name_opt.as_deref().unwrap_or("錐撃"))
                        } else if let Some(ref sn) = skill_name_opt {
                            format!("スキル【{}】(必中強撃)", sn)
                        } else if is_crit {
                            "通常攻撃 [CRITICAL! 防御無視]".to_string()
                        } else {
                            "通常攻撃".to_string()
                        };

                        if final_dmg > 200 || is_sure || is_crit {
                            logs.push(format!(
                                "   ⚔️ [Tick {:>4}] {} の {} -> 『{}』の {} に {} ダメージ!",
                                current_tick, attacker_char.name, act_desc, def_team_name, target_name, final_dmg
                            ));
                        }

                        // オートスペル判定
                        let as_mult = a_stats.autospell_chance_mult;
                        for equip in &attacker_char.equipments {
                            for card in &equip.cards {
                                for modifier in &card.modifiers {
                                    if let Trigger::OnNormalAttackHit { chance } = modifier.trigger {
                                        if rng.gen::<f32>() < (chance * as_mult).min(1.0) {
                                            if let Effect::AutoSpell { ref skill_id, level } = modifier.effect {
                                                if let Some(skill) = self.db.skills.get(skill_id) {
                                                    let mult = skill.magic_multiplier.unwrap_or(level as f32);
                                                    let magic_dmg = ((a_stats.matk as f32) * mult).round() as u32;
                                                    defenders[t_idx].stats.current_hp -= magic_dmg as i32;
                                                    attackers[a_idx].total_damage_dealt += magic_dmg as u64;

                                                    logs.push(format!(
                                                        "   🔮★ [Tick {:>4}] {} の {} がオートスペル発動！ {} に {} の追加魔法ダメージ!",
                                                        current_tick, attacker_char.name, skill.name, target_name, magic_dmg
                                                    ));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        if defenders[t_idx].stats.current_hp <= 0 {
                            defenders[t_idx].is_alive = false;
                            logs.push(format!(
                                "   💀 [Tick {:>4}] 『{}』の {} は力尽き倒れた！",
                                current_tick, def_team_name, target_name
                            ));
                        }
                    }

                    attackers[a_idx].next_action_tick = current_tick + a_stats.attack_interval_ticks;
                }
            }
        }
    }

    /// 空間ノックバック移動処理（障害物や他ユニットとの衝突判定付き）
    fn calculate_knockback_step(
        map: &HexMap,
        current_pos: HexCoord,
        attacker_pos: HexCoord,
        distance: u32,
        occupied: &HashSet<HexCoord>,
    ) -> HexCoord {
        let mut curr = current_pos;
        for _ in 0..distance {
            let next = curr.knockback_target(attacker_pos, 1);
            if map.is_walkable(next) && !occupied.contains(&next) {
                curr = next;
            } else {
                break; // 障害物または他ユニットに衝突し停止
            }
        }
        curr
    }

    /// 空間戦場での戦闘シミュレーション（チョークポイント防衛戦・遠距離ハラスメント・AoE・ノックバック）
    pub fn simulate_spatial_battle(
        &self,
        map: &HexMap,
        party: &[Character],
        monsters: &mut [SpawnedMonster],
    ) -> SpatialBattleResult {
        let mut rng = rand::thread_rng();
        let weapon_mult = self.calculate_party_weapon_multiplier(party);
        let overcharge_mult = self.calculate_overcharge_multiplier(party);

        // 1. 味方キャラクターの空間初期配置（陣形ロールに応じた配置）
        // 隘路（q:4, r:2）を前提とした初期配置オフセット
        let mut combatants: Vec<Combatant> = Vec::new();
        let mut frontline_offset = 0i32;
        let mut midline_offset = 0i32;
        let mut backline_offset = 0i32;

        for ch in party {
            let stats = self.calculate_combat_stats(ch, weapon_mult);
            let attack_range = Self::get_character_attack_range(ch);
            let move_ticks = Self::get_character_move_ticks(ch);

            let initial_pos = match ch.formation.role {
                FormationRole::Frontline => {
                    let pos = HexCoord::new(3, 2 + frontline_offset);
                    frontline_offset += 1;
                    pos
                }
                FormationRole::Midline => {
                    let pos = HexCoord::new(1, 2 + midline_offset);
                    midline_offset += 1;
                    pos
                }
                FormationRole::Backline => {
                    let pos = HexCoord::new(-1, 2 + backline_offset);
                    backline_offset += 1;
                    pos
                }
            };

            combatants.push(Combatant {
                character: ch.clone(),
                stats,
                next_action_tick: 0,
                is_alive: true,
                total_damage_dealt: 0,
                autospell_count: 0,
                position: initial_pos,
                move_ticks,
                next_move_tick: 0,
                attack_range,
            });
        }

        // 2. モンスター群の空間初期配置（隘路の東側から迫る）
        struct SpatialMonster {
            pub monster: SpawnedMonster,
            pub position: HexCoord,
            pub move_ticks: u32,
            pub next_move_tick: u32,
            pub next_action_tick: u32,
            pub attack_interval_ticks: u32,
            pub is_alive: bool,
        }

        let mut spatial_monsters: Vec<SpatialMonster> = Vec::new();
        for (idx, m) in monsters.iter().enumerate() {
            let m_aspd = 140.0 + (m.stats.agi as f32 * 0.2);
            let interval = (((200.0 - m_aspd.min(190.0)) / 2.0).round().max(6.0)) as u32;
            let move_ticks = ((24.0 - (m.stats.agi as f32 * 0.15)).max(7.0).round()) as u32;

            // 隘路の東側（q: 6..8, r: 1..3）にスポーン
            let pos = match idx {
                0 => HexCoord::new(6, 2), // 最前線（隘路直前）
                1 => HexCoord::new(7, 1), // 上側から迫る
                2 => HexCoord::new(7, 3), // 下側から迫る
                _ => HexCoord::new(8 + (idx as i32 / 2), 2 + (idx as i32 % 2)),
            };

            spatial_monsters.push(SpatialMonster {
                monster: m.clone(),
                position: pos,
                move_ticks,
                next_move_tick: 0,
                next_action_tick: 0,
                attack_interval_ticks: interval,
                is_alive: true,
            });
        }

        let mut logs = Vec::new();
        let mut ascii_snapshots = Vec::new();
        let mut current_tick = 0u32;
        let mut total_damage = 0u64;
        let mut snapshot_trigger_count = 0;

        logs.push(format!("🛡️  【空間タクティカル防衛戦開戦】戦場: 『{}』", map.name));
        logs.push(format!(
            "   自軍編成: {}名 | 侵攻モンスター群: {}体",
            combatants.len(),
            spatial_monsters.len()
        ));

        // 開幕時アスキー盤面スナップショットの取得
        {
            let mut display_units = Vec::new();
            for (idx, c) in combatants.iter().enumerate() {
                let sym = match c.character.formation.role {
                    FormationRole::Frontline => format!("P{}", idx + 1),
                    FormationRole::Midline => format!("M{}", idx + 1),
                    FormationRole::Backline => format!("B{}", idx + 1),
                };
                display_units.push(AsciiUnitDisplay {
                    coord: c.position,
                    symbol: sym,
                    is_team_a: true,
                    current_hp: c.stats.current_hp,
                    max_hp: c.stats.max_hp,
                });
            }
            for (idx, m) in spatial_monsters.iter().enumerate() {
                display_units.push(AsciiUnitDisplay {
                    coord: m.position,
                    symbol: format!("E{}", idx + 1),
                    is_team_a: false,
                    current_hp: m.monster.current_hp,
                    max_hp: m.monster.max_hp,
                });
            }
            let snapshot = render_ascii_map(map, &display_units, None);
            ascii_snapshots.push(format!("--- 【開幕時盤面 (Tick 0)】---\n{}", snapshot));
        }

        // 空間戦闘Tickループ
        while combatants.iter().any(|c| c.is_alive)
            && spatial_monsters.iter().any(|m| m.is_alive)
            && current_tick < 10000
        {
            // 全占有ヘクスの収集（他ユニットとの衝突・迂回用）
            let mut occupied_hexes: HashSet<HexCoord> = HashSet::new();
            for c in &combatants {
                if c.is_alive {
                    occupied_hexes.insert(c.position);
                }
            }
            for m in &spatial_monsters {
                if m.is_alive {
                    occupied_hexes.insert(m.position);
                }
            }

            // A. 味方プレイヤー側の空間行動判定
            for c_idx in 0..combatants.len() {
                if !combatants[c_idx].is_alive {
                    continue;
                }

                // 最も近いモンスターをターゲット選定
                let mut best_m_idx = None;
                let mut min_dist = u32::MAX;
                for (m_idx, m) in spatial_monsters.iter().enumerate() {
                    if m.is_alive {
                        let dist = combatants[c_idx].position.distance(m.position);
                        if dist < min_dist {
                            min_dist = dist;
                            best_m_idx = Some(m_idx);
                        }
                    }
                }

                let target_m_idx = match best_m_idx {
                    Some(idx) => idx,
                    None => break,
                };

                let target_pos = spatial_monsters[target_m_idx].position;
                let current_dist = combatants[c_idx].position.distance(target_pos);
                let c_char = combatants[c_idx].character.clone();
                let c_stats = combatants[c_idx].stats.clone();

                // ガンビットルールの空間評価
                let mut chosen_action = Action::NormalAttack;
                let mut sorted_rules = c_char.gambit_rules.clone();
                sorted_rules.sort_by_key(|r| r.priority);

                for rule in &sorted_rules {
                    let matched = match &rule.condition {
                        Condition::TargetArchetypeIs(arch) => spatial_monsters[target_m_idx].monster.archetype == *arch,
                        Condition::TargetRaceIs(race) => spatial_monsters[target_m_idx].monster.race == *race,
                        Condition::SelfHpBelow(ratio) => {
                            (c_stats.current_hp as f32 / c_stats.max_hp as f32) < *ratio
                        }
                        Condition::TargetVitAbove(threshold) => {
                            (spatial_monsters[target_m_idx].monster.stats.vit as u32) >= *threshold
                        }
                        Condition::TargetDistanceAtLeast(dist) => current_dist >= *dist,
                        Condition::TargetDistanceAtMost(dist) => current_dist <= *dist,
                        Condition::EnemiesInRadius { radius, min_count } => {
                            let nearby = spatial_monsters.iter().filter(|m| {
                                m.is_alive && m.position.distance(target_pos) <= *radius
                            }).count();
                            nearby >= *min_count
                        }
                        Condition::Always => true,
                    };
                    if matched {
                        chosen_action = rule.action.clone();
                        break;
                    }
                }

                // アクションに必要な射程の判定
                let (req_range, skill_opt) = match chosen_action {
                    Action::UseActiveSkill(ref sid) => {
                        let sk = self.db.skills.get(sid);
                        let range = sk.map(|s| s.attack_range).unwrap_or(combatants[c_idx].attack_range);
                        (range, sk)
                    }
                    _ => (combatants[c_idx].attack_range, None),
                };

                if current_dist > req_range {
                    // 射程外：A*で射程内へ向けて1ヘクス前進
                    if combatants[c_idx].next_move_tick <= current_tick {
                        let path = HexPathfinder::find_path_to_range(
                            map,
                            combatants[c_idx].position,
                            target_pos,
                            req_range,
                            &occupied_hexes,
                        );
                        if let Some(steps) = path {
                            if let Some(&next_step) = steps.first() {
                                if !occupied_hexes.contains(&next_step) && map.is_walkable(next_step) {
                                    occupied_hexes.remove(&combatants[c_idx].position);
                                    combatants[c_idx].position = next_step;
                                    occupied_hexes.insert(next_step);
                                    combatants[c_idx].next_move_tick = current_tick + combatants[c_idx].move_ticks;
                                }
                            }
                        }
                    }
                } else {
                    // 射程内：攻撃またはスキル発動
                    if combatants[c_idx].next_action_tick <= current_tick {
                        let attacker_pos = combatants[c_idx].position;

                        if let Some(skill) = skill_opt {
                            let is_sure = skill.is_sure_hit;
                            let mult = skill.physical_multiplier.or(skill.magic_multiplier).unwrap_or(1.5);
                            let is_magic = skill.magic_multiplier.is_some();
                            let base_power = if is_magic {
                                c_stats.matk as f32
                            } else {
                                rng.gen_range(c_stats.min_atk..=c_stats.max_atk) as f32
                            };

                            // AoE（範囲攻撃）判定
                            let aoe_targets: Vec<usize> = if let Some(radius) = skill.aoe_radius {
                                let aoe_hexes = HexCoord::get_hexes_in_radius(target_pos, radius);
                                spatial_monsters.iter().enumerate()
                                    .filter(|(_, m)| m.is_alive && aoe_hexes.contains(&m.position))
                                    .map(|(idx, _)| idx)
                                    .collect()
                            } else {
                                vec![target_m_idx]
                            };

                            let mut hit_count = 0;
                            for &m_idx in &aoe_targets {
                                let m = &mut spatial_monsters[m_idx];
                                let hit_rate = if is_sure || is_magic {
                                    100
                                } else {
                                    (80 + c_stats.hit as i32 - (m.monster.level + m.monster.stats.agi as u32) as i32).clamp(10, 95)
                                };
                                let hit_roll: i32 = rng.gen_range(1..=100);
                                if hit_roll <= hit_rate {
                                    let race_mult = self.calculate_race_damage_multiplier(&c_char, m.monster.race);
                                    let m_def = (m.monster.stats.vit as f32) * 0.4;
                                    let raw_dmg = if is_magic {
                                        base_power * mult * race_mult
                                    } else {
                                        (base_power * mult - m_def).max(1.0) * race_mult
                                    };
                                    let final_dmg = raw_dmg.round() as u32 + c_stats.ignore_def_damage;
                                    m.monster.current_hp -= final_dmg as i32;
                                    combatants[c_idx].total_damage_dealt += final_dmg as u64;
                                    total_damage += final_dmg as u64;
                                    hit_count += 1;

                                    if m.monster.current_hp <= 0 {
                                        m.is_alive = false;
                                        occupied_hexes.remove(&m.position);
                                        logs.push(format!(
                                            "   💥 [Tick {:>4}] {} は倒れた！",
                                            current_tick, m.monster.name
                                        ));
                                    }
                                }
                            }

                            logs.push(format!(
                                "   🔥 [Tick {:>4}] {} のスキル【{}】発動！ 標的(q:{}, r:{}) 周囲 {}体に直撃！",
                                current_tick, c_char.name, skill.name, target_pos.q, target_pos.r, hit_count
                            ));

                            // ノックバック処理
                            if let Some(kb_dist) = skill.knockback_distance {
                                if spatial_monsters[target_m_idx].is_alive {
                                    let old_pos = spatial_monsters[target_m_idx].position;
                                    let new_pos = Self::calculate_knockback_step(
                                        map,
                                        old_pos,
                                        attacker_pos,
                                        kb_dist,
                                        &occupied_hexes,
                                    );
                                    if new_pos != old_pos {
                                        occupied_hexes.remove(&old_pos);
                                        spatial_monsters[target_m_idx].position = new_pos;
                                        occupied_hexes.insert(new_pos);
                                        logs.push(format!(
                                            "   💨 [ノックバック] {} は後方へ突き飛ばされた！ ((q:{}, r:{}) -> (q:{}, r:{}))",
                                            spatial_monsters[target_m_idx].monster.name,
                                            old_pos.q, old_pos.r, new_pos.q, new_pos.r
                                        ));
                                    }
                                }
                            }

                            // 大技発動時スナップショットの記録（最初の2回まで）
                            if snapshot_trigger_count < 2 && (skill.aoe_radius.is_some() || skill.knockback_distance.is_some()) {
                                snapshot_trigger_count += 1;
                                let mut display_units = Vec::new();
                                for (idx, c) in combatants.iter().enumerate() {
                                    let sym = match c.character.formation.role {
                                        FormationRole::Frontline => format!("P{}", idx + 1),
                                        FormationRole::Midline => format!("M{}", idx + 1),
                                        FormationRole::Backline => format!("B{}", idx + 1),
                                    };
                                    display_units.push(AsciiUnitDisplay {
                                        coord: c.position,
                                        symbol: sym,
                                        is_team_a: true,
                                        current_hp: c.stats.current_hp,
                                        max_hp: c.stats.max_hp,
                                    });
                                }
                                for (idx, m) in spatial_monsters.iter().enumerate() {
                                    if m.is_alive {
                                        display_units.push(AsciiUnitDisplay {
                                            coord: m.position,
                                            symbol: format!("E{}", idx + 1),
                                            is_team_a: false,
                                            current_hp: m.monster.current_hp,
                                            max_hp: m.monster.max_hp,
                                        });
                                    }
                                }
                                let highlight = skill.aoe_radius.map(|r| {
                                    HexCoord::get_hexes_in_radius(target_pos, r).into_iter().collect::<HashSet<_>>()
                                });
                                let snapshot = render_ascii_map(map, &display_units, highlight.as_ref());
                                ascii_snapshots.push(format!(
                                    "--- 【スキル発動局面 (Tick {})】{} の【{}】---\n{}",
                                    current_tick, c_char.name, skill.name, snapshot
                                ));
                            }
                        } else {
                            // 通常攻撃
                            let m = &mut spatial_monsters[target_m_idx];
                            let hit_rate = (80 + c_stats.hit as i32 - (m.monster.level + m.monster.stats.agi as u32) as i32).clamp(10, 95);
                            let hit_roll: i32 = rng.gen_range(1..=100);
                            let crit_roll: f32 = rng.gen_range(0.0..100.0);
                            let is_crit = crit_roll < (c_stats.crit - (m.monster.stats.luk as f32 * 0.15));

                            if is_crit || hit_roll <= hit_rate {
                                let base_dmg = rng.gen_range(c_stats.min_atk..=c_stats.max_atk) as f32;
                                let race_mult = self.calculate_race_damage_multiplier(&c_char, m.monster.race);
                                let m_def = (m.monster.stats.vit as f32) * 0.4;
                                let raw_dmg = if is_crit {
                                    base_dmg * race_mult * 1.4
                                } else {
                                    (base_dmg - m_def).max(1.0) * race_mult
                                };
                                let final_dmg = raw_dmg.round() as u32 + c_stats.ignore_def_damage;
                                m.monster.current_hp -= final_dmg as i32;
                                combatants[c_idx].total_damage_dealt += final_dmg as u64;
                                total_damage += final_dmg as u64;

                                if m.monster.current_hp <= 0 {
                                    m.is_alive = false;
                                    occupied_hexes.remove(&m.position);
                                    logs.push(format!(
                                        "   💀 [Tick {:>4}] {} は撃破された！",
                                        current_tick, m.monster.name
                                    ));
                                }
                            }
                        }

                        combatants[c_idx].next_action_tick = current_tick + c_stats.attack_interval_ticks;
                    }
                }
            }

            // B. モンスター側の空間行動判定
            for m_idx in 0..spatial_monsters.len() {
                if !spatial_monsters[m_idx].is_alive {
                    continue;
                }

                // 最も近い味方キャラクターをターゲット
                let mut best_c_idx = None;
                let mut min_dist = u32::MAX;
                for (c_idx, c) in combatants.iter().enumerate() {
                    if c.is_alive {
                        let dist = spatial_monsters[m_idx].position.distance(c.position);
                        if dist < min_dist {
                            min_dist = dist;
                            best_c_idx = Some(c_idx);
                        }
                    }
                }

                let target_c_idx = match best_c_idx {
                    Some(idx) => idx,
                    None => break,
                };

                let target_pos = combatants[target_c_idx].position;
                let current_dist = spatial_monsters[m_idx].position.distance(target_pos);
                let m_range = spatial_monsters[m_idx].monster.attack_range;

                if current_dist > m_range {
                    // 射程外：A*で接近（隘路に前衛がいる場合は隘路入口で前衛に阻まれる！）
                    if spatial_monsters[m_idx].next_move_tick <= current_tick {
                        let path = HexPathfinder::find_path_to_range(
                            map,
                            spatial_monsters[m_idx].position,
                            target_pos,
                            m_range,
                            &occupied_hexes,
                        );
                        if let Some(steps) = path {
                            if let Some(&next_step) = steps.first() {
                                if !occupied_hexes.contains(&next_step) && map.is_walkable(next_step) {
                                    occupied_hexes.remove(&spatial_monsters[m_idx].position);
                                    spatial_monsters[m_idx].position = next_step;
                                    occupied_hexes.insert(next_step);
                                    spatial_monsters[m_idx].next_move_tick = current_tick + spatial_monsters[m_idx].move_ticks;
                                }
                            }
                        }
                    }
                } else {
                    // 射程内：攻撃
                    if spatial_monsters[m_idx].next_action_tick <= current_tick {
                        let m = &spatial_monsters[m_idx];
                        let target = &mut combatants[target_c_idx];
                        let m_hit = m.monster.level + m.monster.stats.dex as u32;
                        let hit_rate = (80 + m_hit as i32 - target.stats.flee as i32).clamp(15, 90);
                        let roll: i32 = rng.gen_range(1..=100);

                        if roll <= hit_rate {
                            let m_str = m.monster.stats.str_stat as u32;
                            let atk_min = m_str + (m.monster.stats.dex as u32 / 4);
                            let atk_max = m_str * 2 + (m.monster.stats.str_stat as u32 / 10).pow(2);
                            let raw_atk = rng.gen_range(atk_min..=atk_max) as f32;
                            let def_val = (target.stats.vit_def + target.stats.equip_def) as f32 * 0.45;
                            let dmg = (raw_atk - def_val).max(1.0).round() as u32;

                            target.stats.current_hp -= dmg as i32;
                            if target.stats.current_hp <= 0 {
                                target.is_alive = false;
                                occupied_hexes.remove(&target.position);
                                logs.push(format!(
                                    "   ⚠️ [Tick {:>4}] 『{}』の壁が崩壊！ 倒れた！",
                                    current_tick, target.character.name
                                ));
                            }
                        }

                        spatial_monsters[m_idx].next_action_tick = current_tick + spatial_monsters[m_idx].attack_interval_ticks;
                    }
                }
            }

            current_tick += 1;
        }

        let is_victory = spatial_monsters.iter().all(|m| !m.is_alive);
        let defeated_monsters = spatial_monsters.iter().filter(|m| !m.is_alive).count();

        // 決着時アスキー盤面スナップショットの取得
        {
            let mut display_units = Vec::new();
            for (idx, c) in combatants.iter().enumerate() {
                if c.is_alive {
                    let sym = match c.character.formation.role {
                        FormationRole::Frontline => format!("P{}", idx + 1),
                        FormationRole::Midline => format!("M{}", idx + 1),
                        FormationRole::Backline => format!("B{}", idx + 1),
                    };
                    display_units.push(AsciiUnitDisplay {
                        coord: c.position,
                        symbol: sym,
                        is_team_a: true,
                        current_hp: c.stats.current_hp,
                        max_hp: c.stats.max_hp,
                    });
                }
            }
            for (idx, m) in spatial_monsters.iter().enumerate() {
                if m.is_alive {
                    display_units.push(AsciiUnitDisplay {
                        coord: m.position,
                        symbol: format!("E{}", idx + 1),
                        is_team_a: false,
                        current_hp: m.monster.current_hp,
                        max_hp: m.monster.max_hp,
                    });
                }
            }
            let snapshot = render_ascii_map(map, &display_units, None);
            ascii_snapshots.push(format!("--- 【決着時盤面 (Tick {})】---\n{}", current_tick, snapshot));
        }

        // ドロップと経験値の精算
        let mut drops = Vec::new();
        let mut raw_loot_value = 0u32;
        let mut gained_exp = 0u64;

        if is_victory {
            for m in &spatial_monsters {
                for drop in &m.monster.drops {
                    if rng.gen::<f32>() < drop.drop_rate {
                        raw_loot_value += drop.base_value;
                        drops.push(drop.clone());
                    }
                }
                let base_exp = self.db.monsters.get(&m.monster.template_id).map(|mt| mt.base_exp).unwrap_or(400);
                gained_exp += base_exp as u64;
            }
            logs.push(format!(
                "🎉 【防衛成功】侵攻モンスター群を完全撃退！ (討伐数: {}体, 経過: {} Ticks)",
                defeated_monsters, current_tick
            ));
        } else {
            logs.push(format!(
                "☠️ 【防衛失敗】パーティーは全滅した… (経過: {} Ticks)",
                current_tick
            ));
        }

        let final_loot_value = ((raw_loot_value as f32) * overcharge_mult).round() as u32;

        SpatialBattleResult {
            map_name: map.name.clone(),
            total_ticks: current_tick,
            total_damage,
            defeated_monsters,
            drops,
            raw_loot_value,
            overcharge_mult,
            final_loot_value,
            gained_exp,
            logs,
            ascii_snapshots,
            is_victory,
        }
    }

    /// 空間戦場での模擬GvG対抗戦（陣形激突・A*移動・AoE・ノックバック）
    pub fn simulate_spatial_gvg(
        &self,
        map: &HexMap,
        team_a: &[Character],
        team_b: &[Character],
        name_a: &str,
        name_b: &str,
        town_bonus_a: Option<crate::guild::TownPassiveBonus>,
    ) -> SpatialGvgResult {
        let mut rng = rand::thread_rng();
        let town_atk_bonus = town_bonus_a.as_ref().map(|b| b.extra_weapon_atk_percent).unwrap_or(0.0);
        let weapon_mult_a = self.calculate_party_weapon_multiplier(team_a) + town_atk_bonus;
        let weapon_mult_b = self.calculate_party_weapon_multiplier(team_b);

        // チームAの初期配置（西側）
        let mut combatants_a: Vec<Combatant> = team_a
            .iter()
            .enumerate()
            .map(|(idx, ch)| {
                let stats = self.calculate_combat_stats(ch, weapon_mult_a);
                let attack_range = Self::get_character_attack_range(ch);
                let move_ticks = Self::get_character_move_ticks(ch);
                let pos = match ch.formation.role {
                    FormationRole::Frontline => HexCoord::new(1, 2 + idx as i32),
                    FormationRole::Midline => HexCoord::new(-1, 2 + idx as i32),
                    FormationRole::Backline => HexCoord::new(-2, 1 + idx as i32),
                };
                Combatant {
                    character: ch.clone(),
                    stats,
                    next_action_tick: 0,
                    is_alive: true,
                    total_damage_dealt: 0,
                    autospell_count: 0,
                    position: pos,
                    move_ticks,
                    next_move_tick: 0,
                    attack_range,
                }
            })
            .collect();

        // チームBの初期配置（東側）
        let mut combatants_b: Vec<Combatant> = team_b
            .iter()
            .enumerate()
            .map(|(idx, ch)| {
                let stats = self.calculate_combat_stats(ch, weapon_mult_b);
                let attack_range = Self::get_character_attack_range(ch);
                let move_ticks = Self::get_character_move_ticks(ch);
                let pos = match ch.formation.role {
                    FormationRole::Frontline => HexCoord::new(6, 2 + idx as i32),
                    FormationRole::Midline => HexCoord::new(8, 2 + idx as i32),
                    FormationRole::Backline => HexCoord::new(9, 1 + idx as i32),
                };
                Combatant {
                    character: ch.clone(),
                    stats,
                    next_action_tick: 0,
                    is_alive: true,
                    total_damage_dealt: 0,
                    autospell_count: 0,
                    position: pos,
                    move_ticks,
                    next_move_tick: 0,
                    attack_range,
                }
            })
            .collect();

        let mut logs = Vec::new();
        let mut ascii_snapshots = Vec::new();
        logs.push(format!("🏆 【空間GvG対抗戦開戦】『{}』 VS 『{}』 (戦場: {})", name_a, name_b, map.name));

        // 開幕時スナップショット
        {
            let mut display_units = Vec::new();
            for (idx, c) in combatants_a.iter().enumerate() {
                display_units.push(AsciiUnitDisplay {
                    coord: c.position,
                    symbol: format!("A{}", idx + 1),
                    is_team_a: true,
                    current_hp: c.stats.current_hp,
                    max_hp: c.stats.max_hp,
                });
            }
            for (idx, c) in combatants_b.iter().enumerate() {
                display_units.push(AsciiUnitDisplay {
                    coord: c.position,
                    symbol: format!("B{}", idx + 1),
                    is_team_a: false,
                    current_hp: c.stats.current_hp,
                    max_hp: c.stats.max_hp,
                });
            }
            let snapshot = render_ascii_map(map, &display_units, None);
            ascii_snapshots.push(format!("--- 【空間GvG 開幕陣形 (Tick 0)】---\n{}", snapshot));
        }

        let mut current_tick = 0u32;
        let mut snapshot_count = 0;

        while combatants_a.iter().any(|c| c.is_alive)
            && combatants_b.iter().any(|c| c.is_alive)
            && current_tick < 15000
        {
            // 全占有ヘクス
            let mut occupied: HashSet<HexCoord> = HashSet::new();
            for c in &combatants_a {
                if c.is_alive { occupied.insert(c.position); }
            }
            for c in &combatants_b {
                if c.is_alive { occupied.insert(c.position); }
            }

            // チームAの空間行動
            Self::process_spatial_team_turn(
                &self,
                map,
                &mut combatants_a,
                &mut combatants_b,
                name_a,
                name_b,
                current_tick,
                &mut occupied,
                &mut rng,
                &mut logs,
            );

            // チームBの空間行動
            Self::process_spatial_team_turn(
                &self,
                map,
                &mut combatants_b,
                &mut combatants_a,
                name_b,
                name_a,
                current_tick,
                &mut occupied,
                &mut rng,
                &mut logs,
            );

            // 中盤の激突スナップショット
            if snapshot_count == 0 && current_tick > 30 {
                let any_close = combatants_a.iter().any(|a| {
                    a.is_alive && combatants_b.iter().any(|b| b.is_alive && a.position.distance(b.position) <= 2)
                });
                if any_close {
                    snapshot_count += 1;
                    let mut display_units = Vec::new();
                    for (idx, c) in combatants_a.iter().enumerate() {
                        if c.is_alive {
                            display_units.push(AsciiUnitDisplay {
                                coord: c.position,
                                symbol: format!("A{}", idx + 1),
                                is_team_a: true,
                                current_hp: c.stats.current_hp,
                                max_hp: c.stats.max_hp,
                            });
                        }
                    }
                    for (idx, c) in combatants_b.iter().enumerate() {
                        if c.is_alive {
                            display_units.push(AsciiUnitDisplay {
                                coord: c.position,
                                symbol: format!("B{}", idx + 1),
                                is_team_a: false,
                                current_hp: c.stats.current_hp,
                                max_hp: c.stats.max_hp,
                            });
                        }
                    }
                    let snapshot = render_ascii_map(map, &display_units, None);
                    ascii_snapshots.push(format!("--- 【空間GvG 陣形激突局面 (Tick {})】---\n{}", current_tick, snapshot));
                }
            }

            current_tick += 1;
        }

        let a_alive = combatants_a.iter().any(|c| c.is_alive);
        let b_alive = combatants_b.iter().any(|c| c.is_alive);

        let (winner, loser) = if a_alive && !b_alive {
            (name_a.to_string(), name_b.to_string())
        } else if b_alive && !a_alive {
            (name_b.to_string(), name_a.to_string())
        } else {
            ("引き分け (Time Over)".to_string(), "".to_string())
        };

        // 決着時スナップショット
        {
            let mut display_units = Vec::new();
            for (idx, c) in combatants_a.iter().enumerate() {
                if c.is_alive {
                    display_units.push(AsciiUnitDisplay {
                        coord: c.position,
                        symbol: format!("A{}", idx + 1),
                        is_team_a: true,
                        current_hp: c.stats.current_hp,
                        max_hp: c.stats.max_hp,
                    });
                }
            }
            for (idx, c) in combatants_b.iter().enumerate() {
                if c.is_alive {
                    display_units.push(AsciiUnitDisplay {
                        coord: c.position,
                        symbol: format!("B{}", idx + 1),
                        is_team_a: false,
                        current_hp: c.stats.current_hp,
                        max_hp: c.stats.max_hp,
                    });
                }
            }
            let snapshot = render_ascii_map(map, &display_units, None);
            ascii_snapshots.push(format!("--- 【空間GvG 決着時盤面 (Tick {})】---\n{}", current_tick, snapshot));
        }

        let team_a_survivors: Vec<String> = combatants_a
            .iter()
            .filter(|c| c.is_alive)
            .map(|c| format!("{} (HP: {}/{})", c.character.name, c.stats.current_hp, c.stats.max_hp))
            .collect();

        let team_b_survivors: Vec<String> = combatants_b
            .iter()
            .filter(|c| c.is_alive)
            .map(|c| format!("{} (HP: {}/{})", c.character.name, c.stats.current_hp, c.stats.max_hp))
            .collect();

        logs.push(format!(
            "\n🏁 【空間GvG決着 (Tick {})】勝者: 『{}』！",
            current_tick, winner
        ));

        SpatialGvgResult {
            map_name: map.name.clone(),
            winner_team: winner,
            loser_team: loser,
            total_ticks: current_tick,
            logs,
            ascii_snapshots,
            team_a_survivors,
            team_b_survivors,
        }
    }

    /// 空間GvGにおけるチーム1ターンの移動・攻撃処理
    fn process_spatial_team_turn(
        &self,
        map: &HexMap,
        attackers: &mut [Combatant],
        defenders: &mut [Combatant],
        _atk_team_name: &str,
        def_team_name: &str,
        current_tick: u32,
        occupied: &mut HashSet<HexCoord>,
        rng: &mut rand::rngs::ThreadRng,
        logs: &mut Vec<String>,
    ) {
        for a_idx in 0..attackers.len() {
            if !attackers[a_idx].is_alive {
                continue;
            }

            // 最も近い生存敵をターゲット
            let mut best_d_idx = None;
            let mut min_dist = u32::MAX;
            for (d_idx, d) in defenders.iter().enumerate() {
                if d.is_alive {
                    let dist = attackers[a_idx].position.distance(d.position);
                    if dist < min_dist {
                        min_dist = dist;
                        best_d_idx = Some(d_idx);
                    }
                }
            }

            let t_idx = match best_d_idx {
                Some(idx) => idx,
                None => break,
            };

            let attacker_pos = attackers[a_idx].position;
            let target_pos = defenders[t_idx].position;
            let current_dist = attacker_pos.distance(target_pos);
            let a_char = attackers[a_idx].character.clone();
            let a_stats = attackers[a_idx].stats.clone();

            // ガンビットルールの空間評価
            let mut chosen_skill_id: Option<String> = None;
            for rule in &a_char.gambit_rules {
                let matches = match &rule.condition {
                    Condition::SelfHpBelow(ratio) => {
                        (a_stats.current_hp as f32 / a_stats.max_hp as f32) < *ratio
                    }
                    Condition::TargetVitAbove(threshold) => {
                        (defenders[t_idx].stats.effective_stats.vit as u32) >= *threshold
                    }
                    Condition::TargetDistanceAtLeast(dist) => current_dist >= *dist,
                    Condition::TargetDistanceAtMost(dist) => current_dist <= *dist,
                    Condition::EnemiesInRadius { radius, min_count } => {
                        let count = defenders.iter().filter(|d| d.is_alive && d.position.distance(target_pos) <= *radius).count();
                        count >= *min_count
                    }
                    Condition::Always => true,
                    _ => false,
                };
                if matches {
                    if let Action::UseActiveSkill(ref sid) = rule.action {
                        chosen_skill_id = Some(sid.clone());
                        break;
                    }
                }
            }

            let req_range = if let Some(ref sid) = chosen_skill_id {
                self.db.skills.get(sid).map(|s| s.attack_range).unwrap_or(attackers[a_idx].attack_range)
            } else {
                attackers[a_idx].attack_range
            };

            if current_dist > req_range {
                // 射程外：A*で射程内へ向けて前進
                if attackers[a_idx].next_move_tick <= current_tick {
                    let path = HexPathfinder::find_path_to_range(
                        map,
                        attacker_pos,
                        target_pos,
                        req_range,
                        occupied,
                    );
                    if let Some(steps) = path {
                        if let Some(&next_step) = steps.first() {
                            if !occupied.contains(&next_step) && map.is_walkable(next_step) {
                                occupied.remove(&attacker_pos);
                                attackers[a_idx].position = next_step;
                                occupied.insert(next_step);
                                attackers[a_idx].next_move_tick = current_tick + attackers[a_idx].move_ticks;
                            }
                        }
                    }
                }
            } else {
                // 射程内：攻撃またはスキル発動
                if attackers[a_idx].next_action_tick <= current_tick {
                    let target_name = defenders[t_idx].character.name.clone();

                    // 回復スキルの処理
                    if let Some(ref sid) = chosen_skill_id {
                        if let Some(sk) = self.db.skills.get(sid) {
                            if let Some(heal) = sk.heal_amount {
                                let mut lowest_idx = a_idx;
                                let mut lowest_ratio = attackers[a_idx].stats.current_hp as f32 / attackers[a_idx].stats.max_hp as f32;
                                for (idx, ally) in attackers.iter().enumerate() {
                                    if ally.is_alive {
                                        let r = ally.stats.current_hp as f32 / ally.stats.max_hp as f32;
                                        if r < lowest_ratio {
                                            lowest_ratio = r;
                                            lowest_idx = idx;
                                        }
                                    }
                                }
                                let target_ally_name = attackers[lowest_idx].character.name.clone();
                                attackers[lowest_idx].stats.current_hp = (attackers[lowest_idx].stats.current_hp + heal as i32).min(attackers[lowest_idx].stats.max_hp as i32);
                                logs.push(format!(
                                    "   💚 [Tick {:>4}] {} はスキル【{}】を詠唱！ 『{}』 のHPを {} 回復！",
                                    current_tick, a_char.name, sk.name, target_ally_name, heal
                                ));
                                attackers[a_idx].next_action_tick = current_tick + a_stats.attack_interval_ticks;
                                continue;
                            }
                        }
                    }

                    let skill_ref = chosen_skill_id.as_ref().and_then(|sid| self.db.skills.get(sid));
                    let is_sure = skill_ref.map(|s| s.is_sure_hit).unwrap_or(false);
                    let phys_mult = skill_ref.and_then(|s| s.physical_multiplier).unwrap_or(1.0);
                    let is_piercing = skill_ref.map(|s| s.is_piercing_def).unwrap_or(false);
                    let is_magic = skill_ref.map(|s| s.magic_multiplier.is_some()).unwrap_or(false);
                    let magic_mult = skill_ref.and_then(|s| s.magic_multiplier).unwrap_or(1.0);
                    let skill_name_opt = skill_ref.map(|s| s.name.clone());

                    // AoE巻き込み対象
                    let aoe_targets: Vec<usize> = if let Some(sk) = skill_ref {
                        if let Some(radius) = sk.aoe_radius {
                            let aoe_hexes = HexCoord::get_hexes_in_radius(target_pos, radius);
                            defenders.iter().enumerate()
                                .filter(|(_, d)| d.is_alive && aoe_hexes.contains(&d.position))
                                .map(|(idx, _)| idx)
                                .collect()
                        } else {
                            vec![t_idx]
                        }
                    } else {
                        vec![t_idx]
                    };

                    for &def_idx in &aoe_targets {
                        let cur_d_stats = &defenders[def_idx].stats;
                        let hit_rate = (80 + a_stats.hit as i32 - cur_d_stats.flee as i32).clamp(10, 95);
                        let hit_roll: i32 = rng.gen_range(1..=100);
                        let crit_roll: f32 = rng.gen_range(0.0..100.0);
                        let is_crit = crit_roll < (a_stats.crit - (cur_d_stats.effective_stats.luk as f32 * 0.15));

                        if is_sure || is_crit || is_magic || hit_roll <= hit_rate {
                            let base_dmg = if is_magic {
                                a_stats.matk as f32 * magic_mult
                            } else {
                                rng.gen_range(a_stats.min_atk..=a_stats.max_atk) as f32 * phys_mult
                            };
                            let race_mult = self.calculate_race_damage_multiplier(&a_char, Race::Demihuman);
                            let def_val = (cur_d_stats.vit_def + cur_d_stats.equip_def) as f32 * 0.5;

                            let raw_dmg = if is_magic {
                                base_dmg * race_mult
                            } else if is_crit {
                                base_dmg * race_mult * 1.4
                            } else if is_piercing {
                                let pierce_bonus = (def_val * 4.2).max(60.0);
                                (base_dmg + pierce_bonus) * race_mult
                            } else {
                                (base_dmg - def_val).max(1.0) * race_mult
                            };

                            let final_dmg = raw_dmg.round() as u32 + a_stats.ignore_def_damage;
                            defenders[def_idx].stats.current_hp -= final_dmg as i32;
                            attackers[a_idx].total_damage_dealt += final_dmg as u64;

                            let victim_name = defenders[def_idx].character.name.clone();
                            let action_label = skill_name_opt.as_deref().unwrap_or("通常攻撃");
                            if final_dmg > 200 || is_sure || is_crit || is_magic {
                                logs.push(format!(
                                    "   ⚔️ [Tick {:>4}] {} の【{}】 -> 『{}』の {} に {} ダメージ!",
                                    current_tick, a_char.name, action_label, def_team_name, victim_name, final_dmg
                                ));
                            }

                            if defenders[def_idx].stats.current_hp <= 0 {
                                defenders[def_idx].is_alive = false;
                                occupied.remove(&defenders[def_idx].position);
                                logs.push(format!(
                                    "   💀 [Tick {:>4}] 『{}』の {} は倒れた！",
                                    current_tick, def_team_name, victim_name
                                ));
                            }
                        }
                    }

                    // ノックバック処理
                    if let Some(sk) = skill_ref {
                        if let Some(kb_dist) = sk.knockback_distance {
                            if defenders[t_idx].is_alive {
                                let old_pos = defenders[t_idx].position;
                                let new_pos = Self::calculate_knockback_step(
                                    map,
                                    old_pos,
                                    attacker_pos,
                                    kb_dist,
                                    occupied,
                                );
                                if new_pos != old_pos {
                                    occupied.remove(&old_pos);
                                    defenders[t_idx].position = new_pos;
                                    occupied.insert(new_pos);
                                    logs.push(format!(
                                        "   💨 [GvGノックバック] {} は弾き飛ばされた！ ((q:{}, r:{}) -> (q:{}, r:{}))",
                                        target_name, old_pos.q, old_pos.r, new_pos.q, new_pos.r
                                    ));
                                }
                            }
                        }
                    }

                    attackers[a_idx].next_action_tick = current_tick + a_stats.attack_interval_ticks;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Equipment, TacticalFormation};
    use crate::progression::{GrowthPlan, Progression};

    #[test]
    fn test_spatial_knockback_step() {
        let mut map = HexMap::new("kb_map", "ノックバックテスト", -5, 5, -5, 5);
        let attacker_pos = HexCoord::new(0, 0);
        let target_pos = HexCoord::new(1, 0); // ターゲットは東隣
        let occupied = HashSet::new();

        // 障害物がない場合、東へ2ヘクス押し出されて (3, 0) へ移動
        let new_pos = SimulationEngine::calculate_knockback_step(&map, target_pos, attacker_pos, 2, &occupied);
        assert_eq!(new_pos, HexCoord::new(3, 0));

        // (2, 0) に壁がある場合、壁の手前 (1, 0) で停止
        map.blocked_hexes.insert(HexCoord::new(2, 0));
        let blocked_pos = SimulationEngine::calculate_knockback_step(&map, target_pos, attacker_pos, 2, &occupied);
        assert_eq!(blocked_pos, HexCoord::new(1, 0));
    }

    #[test]
    fn test_spatial_chokepoint_defense_and_aoe() {
        let db = ModDatabase::load_from_dir("mods/core").expect("Failed to load core mods");
        let engine = SimulationEngine::new(&db);
        let map = HexMap::create_chokepoint_map();

        // 1. 前衛壁役（ナイト: バルガス）
        let mut tank = Character::new_with_gen(
            "tank_vargas",
            "壁ナイト・バルガス",
            "swordsman",
            20,
            1,
            Stats::new(30, 15, 60, 10, 20, 10),
        );
        tank.formation = TacticalFormation {
            role: FormationRole::Frontline,
            priority_target_role: None,
        };
        tank.equipments.push(Equipment {
            id: "tank_sword".to_string(),
            name: "重厚なツルギ".to_string(),
            slot: EquipSlot::Weapon,
            weapon_atk: 100,
            attack_range: 1,
            refine: 4,
            slots: 2,
            cards: vec![],
            modifiers: vec![],
            is_locked: false,
        });

        // 2. 後衛魔法アタッカー（ウィザード: ファイアーボール AoE ＆ アローレペル ノックバック所持）
        let mut mage = Character::new_with_gen(
            "mage_elena",
            "後衛魔導エレナ",
            "wizard",
            25,
            1,
            Stats::new(10, 20, 20, 80, 50, 15),
        );
        mage.formation = TacticalFormation {
            role: FormationRole::Backline,
            priority_target_role: None,
        };
        // 杖（射程4）
        mage.equipments.push(Equipment {
            id: "mage_staff".to_string(),
            name: "魔導の杖".to_string(),
            slot: EquipSlot::Weapon,
            weapon_atk: 80,
            attack_range: 4,
            refine: 5,
            slots: 2,
            cards: vec![],
            modifiers: vec![],
            is_locked: false,
        });
        if let Some(fb) = db.skills.get("fire_ball") {
            mage.learned_skills.push(fb.clone());
        }
        // 敵が2ヘクス以内に迫ったらノックバックで弾き飛ばすガンビット
        if let Some(repel) = db.skills.get("arrow_repel") {
            mage.learned_skills.push(repel.clone());
            mage.gambit_rules.push(crate::models::GambitRule {
                id: "repel_rule".to_string(),
                priority: 1,
                condition: Condition::TargetDistanceAtMost(2),
                action: Action::UseActiveSkill("arrow_repel".to_string()),
            });
        }
        // 敵が密集している、または遠いときはファイアーボール
        mage.gambit_rules.push(crate::models::GambitRule {
            id: "aoe_rule".to_string(),
            priority: 2,
            condition: Condition::EnemiesInRadius { radius: 1, min_count: 1 },
            action: Action::UseActiveSkill("fire_ball".to_string()),
        });

        let party = vec![tank, mage];

        // 侵攻モンスター群：重装型ハイオーク ＋ 迅速型オーク
        let template_high_orc = db.monsters.get("high_orc").expect("high_orc exists");
        let template_orc = db.monsters.get("orc_warrior").expect("orc_warrior exists");
        let m1 = engine.spawn_monster(template_high_orc, MonsterArchetype::Heavy);
        let m2 = engine.spawn_monster(template_orc, MonsterArchetype::Swift);
        let mut monsters = vec![m1, m2];

        let result = engine.simulate_spatial_battle(&map, &party, &mut monsters);

        assert!(result.total_ticks > 0);
        assert!(!result.ascii_snapshots.is_empty(), "スナップショットが取得されていること");
        assert!(result.defeated_monsters >= 1, "少なくとも1体以上のモンスターを撃破");
        assert!(result.total_damage > 0);
    }

    #[test]
    fn test_spatial_gvg_simulation() {
        let db = ModDatabase::load_from_dir("mods/core").expect("Failed to load core mods");
        let engine = SimulationEngine::new(&db);
        let map = HexMap::create_gvg_arena_map();

        let member1 = Progression::create_reincarnated_character(
            "carry",
            "アタッカー",
            "thief",
            30,
            GrowthPlan::AgiCrit,
        );
        let mut member2 = Character::new_with_gen(
            "tank",
            "壁ナイト",
            "swordsman",
            30,
            1,
            Stats::new(40, 20, 70, 10, 30, 10),
        );
        member2.formation = TacticalFormation {
            role: FormationRole::Frontline,
            priority_target_role: None,
        };

        let team_a = vec![member1, member2];

        let mut enemy1 = Character::new_with_gen(
            "enemy_knight",
            "敵重騎士",
            "knight",
            32,
            1,
            Stats::new(45, 20, 75, 10, 30, 15),
        );
        enemy1.formation = TacticalFormation {
            role: FormationRole::Frontline,
            priority_target_role: None,
        };

        let team_b = vec![enemy1];

        let result = engine.simulate_spatial_gvg(
            &map,
            &team_a,
            &team_b,
            "自軍テスト隊",
            "敵軍テスト隊",
            None,
        );

        assert!(result.total_ticks > 0);
        assert!(!result.ascii_snapshots.is_empty(), "GvGスナップショットが取得されていること");
        assert!(result.winner_team == "自軍テスト隊" || result.winner_team == "敵軍テスト隊");
    }
}
