use rand::Rng;
use std::collections::{HashMap, HashSet};

use crate::hex_grid::{AsciiUnitDisplay, HexCoord, HexMap, HexPathfinder, render_ascii_map};
use crate::loader::ModDatabase;
use crate::models::{
    Action, Character, Condition, DropItem, Effect, Element, EquipSlot, FormationRole,
    MonsterArchetype, MonsterTemplate, Race, SpawnedMonster, Stats, TacticalFormation, Trigger,
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
    pub race: Race,
    pub archetype: Option<MonsterArchetype>,
    pub drops: Vec<DropItem>,
    pub base_exp: u64,
}

impl Combatant {
    pub fn new_character(
        character: Character,
        stats: CombatStats,
        position: HexCoord,
        attack_range: u32,
        move_ticks: u32,
    ) -> Self {
        Self {
            character,
            stats,
            next_action_tick: 0,
            is_alive: true,
            total_damage_dealt: 0,
            autospell_count: 0,
            position,
            move_ticks,
            next_move_tick: 0,
            attack_range,
            race: Race::Demihuman,
            archetype: None,
            drops: vec![],
            base_exp: 0,
        }
    }

    pub fn from_monster(m: &SpawnedMonster, pos: HexCoord) -> Self {
        let m_aspd = 140.0 + (m.stats.agi as f32 * 0.2);
        let interval = (((200.0 - m_aspd.min(190.0)) / 2.0).round().max(6.0)) as u32;
        let move_ticks = ((24.0 - (m.stats.agi as f32 * 0.15)).max(7.0).round()) as u32;
        let m_str = m.stats.str_stat as u32;
        let atk_min = m_str + (m.stats.dex as u32 / 4);
        let atk_max = m_str * 2 + (m.stats.str_stat as u32 / 10).pow(2);

        let ch = Character {
            id: format!("monster_{}", m.template_id),
            name: format!("{}({})", m.name, m.archetype.name_ja()),
            job_id: "monster".to_string(),
            level: m.level,
            base_stats: m.stats,
            equipments: vec![],
            learned_skills: vec![],
            gambit_rules: vec![],
            generation: 1,
            exp: 0,
            activity_score: 100,
            formation: TacticalFormation::new(FormationRole::Frontline),
        };

        let c_stats = CombatStats {
            max_hp: m.max_hp,
            current_hp: m.current_hp,
            effective_stats: m.stats,
            min_atk: atk_min,
            max_atk: atk_max,
            matk: m.stats.int as u32 * 2,
            hit: m.level + m.stats.dex as u32,
            flee: m.level + m.stats.agi as u32,
            crit: (m.stats.luk as f32 * 0.3).min(50.0),
            aspd: m_aspd,
            attack_interval_ticks: interval,
            equip_def: 0,
            vit_def: (m.stats.vit as f32 * 0.45).round() as u32,
            weapon_atk_mult: 1.0,
            ignore_def_damage: 0,
            autospell_chance_mult: 1.0,
        };

        let base_exp = (m.level as u64 * 35).max(50);

        Self {
            character: ch,
            stats: c_stats,
            next_action_tick: 0,
            is_alive: true,
            total_damage_dealt: 0,
            autospell_count: 0,
            position: pos,
            move_ticks,
            next_move_tick: 0,
            attack_range: m.attack_range,
            race: m.race,
            archetype: Some(m.archetype),
            drops: m.drops.clone(),
            base_exp,
        }
    }
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

/// GUI画面や描画システムへ通知する戦闘中の視覚イベント
#[derive(Debug, Clone, PartialEq)]
pub enum BattleVisualEvent {
    /// ユニットが1ヘクス移動した
    UnitMoved {
        unit_id: String,
        from: HexCoord,
        to: HexCoord,
    },
    /// ダメージが発生した（通常攻撃または単体/範囲スキル）
    DamageDealt {
        attacker_id: String,
        target_id: String,
        target_hex: HexCoord,
        amount: i32,
        is_crit: bool,
        skill_name: Option<String>,
    },
    /// 攻撃がミス（FLEE回避）した
    AttackMissed {
        attacker_id: String,
        target_id: String,
        target_hex: HexCoord,
    },
    /// 範囲攻撃（AoE）が指定ヘクス中心に発動した
    AreaEffectTriggered {
        center: HexCoord,
        radius: u32,
        skill_name: String,
    },
    /// ノックバックにより弾き飛ばされた
    UnitKnockedBack {
        unit_id: String,
        from: HexCoord,
        to: HexCoord,
    },
    /// ユニットが倒れた
    UnitDied {
        unit_id: String,
        coord: HexCoord,
    },
    /// チャット/ログウィンドウに流すメッセージ
    LogMessage(String),
}

/// リアルタイムGUI表示およびステップ実行のための空間戦闘セッション
#[derive(Debug, Clone)]
pub struct SpatialBattleSession {
    pub map: HexMap,
    pub name_a: String,
    pub name_b: String,
    pub team_a: Vec<Combatant>,
    pub team_b: Vec<Combatant>,
    pub current_tick: u32,
    pub is_finished: bool,
    pub is_victory: bool,
    pub winner_team: Option<String>,
    pub total_damage: u64,
    pub logs: Vec<String>,
    pub max_ticks: u32,
    pub is_monster_battle: bool,
    pub defeated_monsters: usize,
    pub drops: Vec<DropItem>,
    pub raw_loot_value: u32,
    pub final_loot_value: u32,
    pub gained_exp: u64,
    pub overcharge_mult: f32,
    pub snapshot_trigger_count: u32,
    pub ascii_snapshots: Vec<String>,
}

impl SpatialBattleSession {
    /// 新規空間戦闘セッションを生成
    pub fn new(
        map: HexMap,
        team_a: Vec<Combatant>,
        team_b: Vec<Combatant>,
        name_a: String,
        name_b: String,
        overcharge_mult: f32,
        is_monster_battle: bool,
    ) -> Self {
        let max_ticks = if is_monster_battle { 1200 } else { 1500 };
        Self {
            map,
            name_a,
            name_b,
            team_a,
            team_b,
            current_tick: 0,
            is_finished: false,
            is_victory: false,
            winner_team: None,
            total_damage: 0,
            logs: Vec::new(),
            max_ticks,
            is_monster_battle,
            defeated_monsters: 0,
            drops: Vec::new(),
            raw_loot_value: 0,
            final_loot_value: 0,
            gained_exp: 0,
            overcharge_mult,
            snapshot_trigger_count: 0,
            ascii_snapshots: Vec::new(),
        }
    }

    /// 1 Tick分だけ戦闘を進め、そのTickで発生した視覚イベント一覧を返す
    pub fn step_tick(&mut self, db: &ModDatabase) -> Vec<BattleVisualEvent> {
        if self.is_finished {
            return vec![];
        }

        self.current_tick += 1;
        let mut rng = rand::thread_rng();
        let mut events = Vec::new();

        // 生存全ユニットの占有ヘクス集合
        let mut occupied_hexes: HashSet<HexCoord> = HashSet::new();
        for c in &self.team_a {
            if c.is_alive {
                occupied_hexes.insert(c.position);
            }
        }
        for c in &self.team_b {
            if c.is_alive {
                occupied_hexes.insert(c.position);
            }
        }

        // 1. Team A のターン
        Self::process_team_turn(
            self.current_tick,
            &self.map,
            &mut self.team_a,
            &mut self.team_b,
            &mut occupied_hexes,
            &mut self.total_damage,
            &mut self.logs,
            &mut events,
            db,
            &mut rng,
        );

        // 2. Team B のターン
        Self::process_team_turn(
            self.current_tick,
            &self.map,
            &mut self.team_b,
            &mut self.team_a,
            &mut occupied_hexes,
            &mut self.total_damage,
            &mut self.logs,
            &mut events,
            db,
            &mut rng,
        );

        // 3. 決着判定
        let a_alive = self.team_a.iter().any(|c| c.is_alive);
        let b_alive = self.team_b.iter().any(|c| c.is_alive);

        if !b_alive {
            self.is_finished = true;
            self.is_victory = true;
            self.winner_team = Some(self.name_a.clone());
            self.defeated_monsters = self.team_b.len();
            let msg = if self.is_monster_battle {
                format!(
                    "🎉 【防衛成功】侵攻モンスター群を完全撃退！ (討伐数: {}体, 経過: {} Ticks)",
                    self.team_b.len(),
                    self.current_tick
                )
            } else {
                format!(
                    "🏁 【空間GvG決着 (Tick {})】勝者: 『{}』！",
                    self.current_tick, self.name_a
                )
            };
            self.logs.push(msg.clone());
            events.push(BattleVisualEvent::LogMessage(msg));
            self.finalize_rewards(&mut rng);
        } else if !a_alive {
            self.is_finished = true;
            self.is_victory = false;
            self.winner_team = Some(self.name_b.clone());
            let msg = if self.is_monster_battle {
                format!("☠️ 【防衛失敗】パーティーは全滅した… (経過: {} Ticks)", self.current_tick)
            } else {
                format!(
                    "🏁 【空間GvG決着 (Tick {})】勝者: 『{}』！",
                    self.current_tick, self.name_b
                )
            };
            self.logs.push(msg.clone());
            events.push(BattleVisualEvent::LogMessage(msg));
        } else if self.current_tick >= self.max_ticks {
            self.is_finished = true;
            self.is_victory = false;
            self.winner_team = Some("時間切れ引き分け".to_string());
            let msg = format!("⏱️ 【時間切れ引き分け】最大所要時間を超過しました (経過: {} Ticks)", self.current_tick);
            self.logs.push(msg.clone());
            events.push(BattleVisualEvent::LogMessage(msg));
        }

        events
    }

    /// チームごとの行動処理（移動、スキル、通常攻撃、AoE、ノックバック）
    fn process_team_turn(
        current_tick: u32,
        map: &HexMap,
        actors: &mut [Combatant],
        targets: &mut [Combatant],
        occupied_hexes: &mut HashSet<HexCoord>,
        total_damage: &mut u64,
        logs: &mut Vec<String>,
        events: &mut Vec<BattleVisualEvent>,
        db: &ModDatabase,
        rng: &mut rand::rngs::ThreadRng,
    ) {
        for a_idx in 0..actors.len() {
            if !actors[a_idx].is_alive {
                continue;
            }

            // ターゲット選定（最寄りの生存敵ユニット）
            let mut best_target_idx = None;
            let mut min_dist = u32::MAX;
            let priority_role = actors[a_idx].character.formation.priority_target_role;

            for (t_idx, t) in targets.iter().enumerate() {
                if t.is_alive {
                    let dist = actors[a_idx].position.distance(t.position);
                    let role_matched = priority_role.map_or(false, |r| t.character.formation.role == r);
                    if role_matched && dist <= min_dist + 2 {
                        best_target_idx = Some(t_idx);
                        break;
                    }
                    if dist < min_dist {
                        min_dist = dist;
                        best_target_idx = Some(t_idx);
                    }
                }
            }

            let target_idx = match best_target_idx {
                Some(idx) => idx,
                None => break, // 敵が全滅している
            };

            let target_pos = targets[target_idx].position;
            let current_dist = actors[a_idx].position.distance(target_pos);
            let actor_id = actors[a_idx].character.id.clone();
            let actor_name = actors[a_idx].character.name.clone();
            let actor_pos = actors[a_idx].position;

            // ガンビットルールの評価
            let mut chosen_action = Action::NormalAttack;
            let mut sorted_rules = actors[a_idx].character.gambit_rules.clone();
            sorted_rules.sort_by_key(|r| r.priority);

            for rule in &sorted_rules {
                let matched = match &rule.condition {
                    Condition::TargetArchetypeIs(arch) => targets[target_idx].archetype == Some(*arch),
                    Condition::TargetRaceIs(race) => targets[target_idx].race == *race,
                    Condition::SelfHpBelow(ratio) => {
                        (actors[a_idx].stats.current_hp as f32 / actors[a_idx].stats.max_hp as f32) < *ratio
                    }
                    Condition::TargetVitAbove(threshold) => {
                        (targets[target_idx].stats.effective_stats.vit as u32) >= *threshold
                    }
                    Condition::TargetDistanceAtLeast(dist) => current_dist >= *dist,
                    Condition::TargetDistanceAtMost(dist) => current_dist <= *dist,
                    Condition::EnemiesInRadius { radius, min_count } => {
                        let nearby = targets.iter().filter(|t| {
                            t.is_alive && t.position.distance(target_pos) <= *radius
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

            // 必要射程の決定
            let (req_range, skill_opt) = match chosen_action {
                Action::UseActiveSkill(ref sid) => {
                    let sk = db.skills.get(sid);
                    let range = sk.map(|s| s.attack_range).unwrap_or(actors[a_idx].attack_range);
                    (range, sk)
                }
                _ => (actors[a_idx].attack_range, None),
            };

            if current_dist > req_range {
                // 射程外：A*で射程内へ向けて1ヘクス前進
                if actors[a_idx].next_move_tick <= current_tick {
                    let path = HexPathfinder::find_path_to_range(
                        map,
                        actors[a_idx].position,
                        target_pos,
                        req_range,
                        occupied_hexes,
                    );
                    if let Some(steps) = path {
                        if let Some(&next_step) = steps.first() {
                            if !occupied_hexes.contains(&next_step) && map.is_walkable(next_step) {
                                occupied_hexes.remove(&actors[a_idx].position);
                                let old_pos = actors[a_idx].position;
                                actors[a_idx].position = next_step;
                                occupied_hexes.insert(next_step);
                                actors[a_idx].next_move_tick = current_tick + actors[a_idx].move_ticks;

                                events.push(BattleVisualEvent::UnitMoved {
                                    unit_id: actor_id.clone(),
                                    from: old_pos,
                                    to: next_step,
                                });
                            }
                        }
                    }
                }
            } else {
                // 射程内：攻撃またはスキル発動
                if actors[a_idx].next_action_tick <= current_tick {
                    let c_stats = actors[a_idx].stats.clone();

                    if let Some(skill) = skill_opt {
                        // 回復スキルの処理
                        if let Some(heal) = skill.heal_amount {
                            let mut lowest_idx = a_idx;
                            let mut lowest_ratio = actors[a_idx].stats.current_hp as f32 / actors[a_idx].stats.max_hp as f32;
                            for (idx, ally) in actors.iter().enumerate() {
                                if ally.is_alive {
                                    let r = ally.stats.current_hp as f32 / ally.stats.max_hp as f32;
                                    if r < lowest_ratio {
                                        lowest_ratio = r;
                                        lowest_idx = idx;
                                    }
                                }
                            }
                            let target_ally_name = actors[lowest_idx].character.name.clone();
                            let target_ally_pos = actors[lowest_idx].position;
                            actors[lowest_idx].stats.current_hp = (actors[lowest_idx].stats.current_hp + heal as i32).min(actors[lowest_idx].stats.max_hp as i32);
                            let log_msg = format!(
                                "   💚 [Tick {:>4}] {} はスキル【{}】を詠唱！ 『{}』 のHPを {} 回復！",
                                current_tick, actor_name, skill.name, target_ally_name, heal
                            );
                            logs.push(log_msg.clone());
                            events.push(BattleVisualEvent::LogMessage(log_msg));
                            events.push(BattleVisualEvent::DamageDealt {
                                attacker_id: actor_id.clone(),
                                target_id: actors[lowest_idx].character.id.clone(),
                                target_hex: target_ally_pos,
                                amount: -(heal as i32),
                                is_crit: false,
                                skill_name: Some(skill.name.clone()),
                            });
                            actors[a_idx].next_action_tick = current_tick + actors[a_idx].stats.attack_interval_ticks;
                            continue;
                        }

                        let is_sure = skill.is_sure_hit;
                        let mult = skill.physical_multiplier.or(skill.magic_multiplier).unwrap_or(1.5);
                        let is_magic = skill.magic_multiplier.is_some();
                        let is_piercing = skill.is_piercing_def;
                        let base_power = if is_magic {
                            c_stats.matk as f32
                        } else {
                            rng.gen_range(c_stats.min_atk..=c_stats.max_atk) as f32
                        };

                        // 範囲攻撃（AoE）判定
                        let aoe_targets: Vec<usize> = if let Some(radius) = skill.aoe_radius {
                            let aoe_hexes = HexCoord::get_hexes_in_radius(target_pos, radius);
                            events.push(BattleVisualEvent::AreaEffectTriggered {
                                center: target_pos,
                                radius,
                                skill_name: skill.name.clone(),
                            });
                            targets.iter().enumerate()
                                .filter(|(_, t)| t.is_alive && aoe_hexes.contains(&t.position))
                                .map(|(idx, _)| idx)
                                .collect()
                        } else {
                            vec![target_idx]
                        };

                        let mut hit_count = 0;
                        for &t_idx in &aoe_targets {
                            let t = &mut targets[t_idx];
                            let hit_rate = if is_sure || is_magic {
                                100
                            } else {
                                (80 + c_stats.hit as i32 - t.stats.flee as i32).clamp(10, 95)
                            };
                            let hit_roll: i32 = rng.gen_range(1..=100);
                            if hit_roll <= hit_rate {
                                let race_mult = 1.0;
                                let t_def = (t.stats.vit_def + t.stats.equip_def) as f32 * 0.45;
                                let raw_dmg = if is_magic {
                                    base_power * mult * race_mult
                                } else if is_piercing {
                                    let pierce_bonus = (t_def * 4.2).max(60.0);
                                    (base_power + pierce_bonus) * race_mult
                                } else {
                                    (base_power * mult - t_def).max(1.0) * race_mult
                                };
                                let final_dmg = raw_dmg.round() as u32 + c_stats.ignore_def_damage;
                                t.stats.current_hp -= final_dmg as i32;
                                actors[a_idx].total_damage_dealt += final_dmg as u64;
                                *total_damage += final_dmg as u64;
                                hit_count += 1;

                                events.push(BattleVisualEvent::DamageDealt {
                                    attacker_id: actor_id.clone(),
                                    target_id: t.character.id.clone(),
                                    target_hex: t.position,
                                    amount: final_dmg as i32,
                                    is_crit: false,
                                    skill_name: Some(skill.name.clone()),
                                });

                                if t.stats.current_hp <= 0 {
                                    t.is_alive = false;
                                    occupied_hexes.remove(&t.position);
                                    let log_msg = format!("   💀 [Tick {:>4}] {} は倒れた！", current_tick, t.character.name);
                                    logs.push(log_msg.clone());
                                    events.push(BattleVisualEvent::LogMessage(log_msg));
                                    events.push(BattleVisualEvent::UnitDied {
                                        unit_id: t.character.id.clone(),
                                        coord: t.position,
                                    });
                                }
                            } else {
                                events.push(BattleVisualEvent::AttackMissed {
                                    attacker_id: actor_id.clone(),
                                    target_id: t.character.id.clone(),
                                    target_hex: t.position,
                                });
                            }
                        }

                        let log_msg = format!(
                            "   🔥 [Tick {:>4}] {} のスキル【{}】発動！ 標的(q:{}, r:{}) 周囲 {}体に直撃！",
                            current_tick, actor_name, skill.name, target_pos.q, target_pos.r, hit_count
                        );
                        logs.push(log_msg.clone());
                        events.push(BattleVisualEvent::LogMessage(log_msg));

                        // ノックバック処理
                        if let Some(kb_dist) = skill.knockback_distance {
                            if targets[target_idx].is_alive {
                                let old_t_pos = targets[target_idx].position;
                                let new_t_pos = SimulationEngine::calculate_knockback_step(
                                    map,
                                    old_t_pos,
                                    actor_pos,
                                    kb_dist,
                                    occupied_hexes,
                                );
                                if new_t_pos != old_t_pos {
                                    occupied_hexes.remove(&old_t_pos);
                                    targets[target_idx].position = new_t_pos;
                                    occupied_hexes.insert(new_t_pos);

                                    let log_msg = format!(
                                        "   💨 [ノックバック] {} は後方へ突き飛ばされた！ ((q:{}, r:{}) -> (q:{}, r:{}))",
                                        targets[target_idx].character.name,
                                        old_t_pos.q, old_t_pos.r, new_t_pos.q, new_t_pos.r
                                    );
                                    logs.push(log_msg.clone());
                                    events.push(BattleVisualEvent::LogMessage(log_msg));
                                    events.push(BattleVisualEvent::UnitKnockedBack {
                                        unit_id: targets[target_idx].character.id.clone(),
                                        from: old_t_pos,
                                        to: new_t_pos,
                                    });
                                }
                            }
                        }
                    } else {
                        // 通常攻撃
                        let t = &mut targets[target_idx];
                        let is_crit = rng.gen_range(0.0..100.0) < c_stats.crit;
                        let hit_rate = if is_crit {
                            100
                        } else {
                            (80 + c_stats.hit as i32 - t.stats.flee as i32).clamp(10, 95)
                        };
                        let hit_roll: i32 = rng.gen_range(1..=100);

                        if hit_roll <= hit_rate {
                            let raw_atk = rng.gen_range(c_stats.min_atk..=c_stats.max_atk) as f32;
                            let def_val = if is_crit { 0.0 } else { (t.stats.vit_def + t.stats.equip_def) as f32 * 0.45 };
                            let final_dmg = (raw_atk - def_val).max(1.0).round() as u32 + c_stats.ignore_def_damage;

                            t.stats.current_hp -= final_dmg as i32;
                            actors[a_idx].total_damage_dealt += final_dmg as u64;
                            *total_damage += final_dmg as u64;

                            events.push(BattleVisualEvent::DamageDealt {
                                attacker_id: actor_id.clone(),
                                target_id: t.character.id.clone(),
                                target_hex: t.position,
                                amount: final_dmg as i32,
                                is_crit,
                                skill_name: None,
                            });

                            if t.stats.current_hp <= 0 {
                                t.is_alive = false;
                                occupied_hexes.remove(&t.position);
                                let log_msg = format!("   💀 [Tick {:>4}] {} は倒れた！", current_tick, t.character.name);
                                logs.push(log_msg.clone());
                                events.push(BattleVisualEvent::LogMessage(log_msg));
                                events.push(BattleVisualEvent::UnitDied {
                                    unit_id: t.character.id.clone(),
                                    coord: t.position,
                                });
                            }
                        } else {
                            events.push(BattleVisualEvent::AttackMissed {
                                attacker_id: actor_id.clone(),
                                target_id: t.character.id.clone(),
                                target_hex: t.position,
                            });
                        }
                    }

                    actors[a_idx].next_action_tick = current_tick + actors[a_idx].stats.attack_interval_ticks;
                }
            }
        }
    }

    /// 勝利時のドロップ品と経験値の精算
    fn finalize_rewards(&mut self, rng: &mut rand::rngs::ThreadRng) {
        if self.is_monster_battle && self.is_victory {
            for m in &self.team_b {
                for drop in &m.drops {
                    if rng.gen::<f32>() < drop.drop_rate {
                        self.raw_loot_value += drop.base_value;
                        self.drops.push(drop.clone());
                    }
                }
                self.gained_exp += m.base_exp;
            }
            self.final_loot_value = (self.raw_loot_value as f32 * self.overcharge_mult).round() as u32;
        }
    }

    /// CUIコンソール表示用のアスキーアート盤面スナップショットを取得
    pub fn get_ascii_snapshot(&self, highlight_hexes: Option<&HashSet<HexCoord>>) -> String {
        let mut display_units = Vec::new();
        for (idx, c) in self.team_a.iter().enumerate() {
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
        for (idx, c) in self.team_b.iter().enumerate() {
            if c.is_alive {
                let sym = if self.is_monster_battle {
                    format!("E{}", idx + 1)
                } else {
                    format!("B{}", idx + 1)
                };
                display_units.push(AsciiUnitDisplay {
                    coord: c.position,
                    symbol: sym,
                    is_team_a: false,
                    current_hp: c.stats.current_hp,
                    max_hp: c.stats.max_hp,
                });
            }
        }
        render_ascii_map(&self.map, &display_units, highlight_hexes)
    }
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
                Combatant::new_character(
                    ch.clone(),
                    stats,
                    HexCoord::new(0, 0),
                    attack_range,
                    move_ticks,
                )
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
                Combatant::new_character(
                    ch.clone(),
                    stats,
                    HexCoord::new(0, 0),
                    attack_range,
                    move_ticks,
                )
            })
            .collect();

        let mut combatants_b: Vec<Combatant> = team_b
            .iter()
            .map(|ch| {
                let stats = self.calculate_combat_stats(ch, weapon_mult_b);
                let attack_range = Self::get_character_attack_range(ch);
                let move_ticks = Self::get_character_move_ticks(ch);
                Combatant::new_character(
                    ch.clone(),
                    stats,
                    HexCoord::new(0, 0),
                    attack_range,
                    move_ticks,
                )
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

    /// 監視塔の隘路マップでの戦闘セッションを生成
    pub fn create_chokepoint_session(
        &self,
        map: HexMap,
        party: &[Character],
        monsters: &[SpawnedMonster],
    ) -> SpatialBattleSession {
        let weapon_mult = self.calculate_party_weapon_multiplier(party);
        let overcharge_mult = self.calculate_overcharge_multiplier(party);

        let mut team_a: Vec<Combatant> = Vec::new();
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

            team_a.push(Combatant::new_character(
                ch.clone(),
                stats,
                initial_pos,
                attack_range,
                move_ticks,
            ));
        }

        let mut team_b: Vec<Combatant> = Vec::new();
        for (i, m) in monsters.iter().enumerate() {
            let initial_pos = HexCoord::new(6 + (i as i32 / 2), 1 + (i as i32 % 3));
            team_b.push(Combatant::from_monster(m, initial_pos));
        }

        SpatialBattleSession::new(
            map,
            team_a,
            team_b,
            "プレイヤー防衛隊".to_string(),
            "侵攻モンスター群".to_string(),
            overcharge_mult,
            true,
        )
    }

    /// GvG闘技場マップでの戦闘セッションを生成
    pub fn create_gvg_session(
        &self,
        map: HexMap,
        team_a: &[Character],
        team_b: &[Character],
        name_a: &str,
        name_b: &str,
        town_bonus_a: Option<crate::guild::TownPassiveBonus>,
    ) -> SpatialBattleSession {
        let town_atk_bonus = town_bonus_a.as_ref().map(|b| b.extra_weapon_atk_percent).unwrap_or(0.0);
        let weapon_mult_a = self.calculate_party_weapon_multiplier(team_a) + town_atk_bonus;
        let weapon_mult_b = self.calculate_party_weapon_multiplier(team_b);
        let overcharge_mult = self.calculate_overcharge_multiplier(team_a);

        // チームA（西側配置）
        let mut frontline_offset_a = 0i32;
        let mut midline_offset_a = 0i32;
        let mut backline_offset_a = 0i32;
        let mut combatants_a: Vec<Combatant> = Vec::new();

        for ch in team_a {
            let stats = self.calculate_combat_stats(ch, weapon_mult_a);
            let attack_range = Self::get_character_attack_range(ch);
            let move_ticks = Self::get_character_move_ticks(ch);

            let initial_pos = match ch.formation.role {
                FormationRole::Frontline => {
                    let pos = HexCoord::new(-2, 0 + frontline_offset_a);
                    frontline_offset_a += 1;
                    pos
                }
                FormationRole::Midline => {
                    let pos = HexCoord::new(-4, 0 + midline_offset_a);
                    midline_offset_a += 1;
                    pos
                }
                FormationRole::Backline => {
                    let pos = HexCoord::new(-6, 0 + backline_offset_a);
                    backline_offset_a += 1;
                    pos
                }
            };

            combatants_a.push(Combatant::new_character(
                ch.clone(),
                stats,
                initial_pos,
                attack_range,
                move_ticks,
            ));
        }

        // チームB（東側配置）
        let mut frontline_offset_b = 0i32;
        let mut midline_offset_b = 0i32;
        let mut backline_offset_b = 0i32;
        let mut combatants_b: Vec<Combatant> = Vec::new();

        for ch in team_b {
            let stats = self.calculate_combat_stats(ch, weapon_mult_b);
            let attack_range = Self::get_character_attack_range(ch);
            let move_ticks = Self::get_character_move_ticks(ch);

            let initial_pos = match ch.formation.role {
                FormationRole::Frontline => {
                    let pos = HexCoord::new(2, 0 + frontline_offset_b);
                    frontline_offset_b += 1;
                    pos
                }
                FormationRole::Midline => {
                    let pos = HexCoord::new(4, 0 + midline_offset_b);
                    midline_offset_b += 1;
                    pos
                }
                FormationRole::Backline => {
                    let pos = HexCoord::new(6, 0 + backline_offset_b);
                    backline_offset_b += 1;
                    pos
                }
            };

            combatants_b.push(Combatant::new_character(
                ch.clone(),
                stats,
                initial_pos,
                attack_range,
                move_ticks,
            ));
        }

        SpatialBattleSession::new(
            map,
            combatants_a,
            combatants_b,
            name_a.to_string(),
            name_b.to_string(),
            overcharge_mult,
            false,
        )
    }

    /// 空間戦場での戦闘シミュレーション（チョークポイント防衛戦・遠距離ハラスメント・AoE・ノックバック）
    pub fn simulate_spatial_battle(
        &self,
        map: &HexMap,
        party: &[Character],
        monsters: &mut [SpawnedMonster],
    ) -> SpatialBattleResult {
        let mut session = self.create_chokepoint_session(map.clone(), party, monsters);
        session.ascii_snapshots.push(session.get_ascii_snapshot(None));

        while !session.is_finished {
            if session.current_tick > 0 && session.current_tick % 150 == 0 {
                session.ascii_snapshots.push(session.get_ascii_snapshot(None));
            }
            session.step_tick(self.db);
        }

        // 撃破されたモンスターのHPを反映
        for (i, m) in monsters.iter_mut().enumerate() {
            if i < session.team_b.len() {
                m.current_hp = session.team_b[i].stats.current_hp;
            }
        }

        SpatialBattleResult {
            map_name: session.map.name.clone(),
            total_ticks: session.current_tick,
            total_damage: session.total_damage,
            defeated_monsters: session.defeated_monsters,
            drops: session.drops,
            raw_loot_value: session.raw_loot_value,
            overcharge_mult: session.overcharge_mult,
            final_loot_value: session.final_loot_value,
            gained_exp: session.gained_exp,
            logs: session.logs,
            ascii_snapshots: session.ascii_snapshots,
            is_victory: session.is_victory,
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
        let mut session = self.create_gvg_session(map.clone(), team_a, team_b, name_a, name_b, town_bonus_a);
        session.ascii_snapshots.push(session.get_ascii_snapshot(None));

        while !session.is_finished {
            if session.current_tick > 0 && session.current_tick % 200 == 0 {
                session.ascii_snapshots.push(session.get_ascii_snapshot(None));
            }
            session.step_tick(self.db);
        }

        let team_a_survivors: Vec<String> = session.team_a.iter().filter(|c| c.is_alive).map(|c| c.character.name.clone()).collect();
        let team_b_survivors: Vec<String> = session.team_b.iter().filter(|c| c.is_alive).map(|c| c.character.name.clone()).collect();

        let (winner, loser) = match session.winner_team.as_deref() {
            Some(w) if w == name_a => (name_a.to_string(), name_b.to_string()),
            Some(w) if w == name_b => (name_b.to_string(), name_a.to_string()),
            _ => {
                if team_a_survivors.len() >= team_b_survivors.len() {
                    (name_a.to_string(), name_b.to_string())
                } else {
                    (name_b.to_string(), name_a.to_string())
                }
            }
        };

        SpatialGvgResult {
            map_name: session.map.name.clone(),
            winner_team: winner,
            loser_team: loser,
            total_ticks: session.current_tick,
            logs: session.logs,
            ascii_snapshots: session.ascii_snapshots,
            team_a_survivors,
            team_b_survivors,
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
