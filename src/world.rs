use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use rand::Rng;

use crate::models::{Character, Equipment, EquipSlot, Stats, StatType};
use crate::loader::ModDatabase;
use crate::progression::{GrowthPlan, Progression};
use crate::guild::{GuildRoster, RivalGuild};
use crate::market::MarketSystem;
use crate::dispatch::{ActiveDispatch, DispatchSystem};

/// ゲームの全状態を集約するワールドステート
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldState {
    /// 現在の世代数
    pub generation: u32,
    /// 経過総Tick（ワールドカレンダー）
    pub calendar_tick: u32,
    /// ギルド資金（Zeny）
    pub treasury_zeny: u32,
    /// 共有倉庫：装備品
    pub storage_equipments: Vec<Equipment>,
    /// 共有倉庫：一般アイテム・素材・カード（アイテムID -> 個数）
    pub storage_items: HashMap<String, u32>,
    /// 現役育成中キャラクター
    pub active_character: Character,
    /// 現役キャラのビルド育成方針
    pub active_growth_plan: GrowthPlan,
    /// 現在の同行パーティー編成（最大2名、現役と合わせて最大3名）
    pub party_members: Vec<Character>,
    /// ギルド名簿（控えメンバー、街NPC、移籍者）
    pub roster: GuildRoster,
    /// ライバルギルド群（リーグ参加勢力）
    pub rival_guilds: Vec<RivalGuild>,
    /// 動的市場システム
    pub market: MarketSystem,
    /// 稼働中の自動派遣部隊
    pub active_dispatch: Option<ActiveDispatch>,
    /// 裏リーグ戦やワールドイベントの最新ログ
    pub rival_league_logs: Vec<String>,
}

impl WorldState {
    /// 新規ゲームの初期ワールドステート生成
    pub fn new_game(db: &ModDatabase) -> Self {
        // 初期第1世代キャラクター：クロード（シーフ）
        let mut active_char = Progression::create_reincarnated_character(
            "char_gen1_carry",
            "クロード",
            "thief",
            1,
            GrowthPlan::AgiCrit,
        );
        // クロードの初期装備
        active_char.equipments.push(Equipment {
            id: "dagger_claude".to_string(),
            name: "マインゴーシュ".to_string(),
            slot: EquipSlot::Weapon,
            weapon_atk: 50,
            refine: 0,
            slots: 4,
            cards: vec![],
            modifiers: vec![],
            is_locked: false,
        });

        // 初期同行メンバー：バルガス（ナイト・壁役）
        let vargas = Character {
            id: "char_vargas".to_string(),
            name: "バルガス".to_string(),
            job_id: "swordsman".to_string(),
            level: 15,
            base_stats: Stats::new(25, 10, 40, 10, 15, 10),
            equipments: vec![Equipment {
                id: "sword_vargas".to_string(),
                name: "ツルギ".to_string(),
                slot: EquipSlot::Weapon,
                weapon_atk: 100,
                refine: 4,
                slots: 2,
                cards: vec![],
                modifiers: vec![],
                is_locked: true,
            }],
            learned_skills: vec![],
            gambit_rules: vec![],
            generation: 1,
            exp: 0,
            activity_score: 100,
        };

        // 初期同行メンバー：オットー（ブラックスミス・支援役）
        let otto = Character {
            id: "char_otto".to_string(),
            name: "オットー".to_string(),
            job_id: "blacksmith".to_string(),
            level: 15,
            base_stats: Stats::new(20, 10, 20, 10, 35, 30),
            equipments: vec![],
            learned_skills: vec![],
            gambit_rules: vec![],
            generation: 1,
            exp: 0,
            activity_score: 100,
        };

        // ギルド名簿の初期化（最大定員10名）
        let mut roster = GuildRoster::new(10);
        roster.treasury_zeny = 50_000;

        // 控えメンバーの追加
        let reserve1 = Character {
            id: "char_reserve_merchant".to_string(),
            name: "エミリア".to_string(),
            job_id: "merchant".to_string(),
            level: 12,
            base_stats: Stats::new(10, 30, 15, 10, 35, 10),
            equipments: vec![],
            learned_skills: vec![],
            gambit_rules: vec![],
            generation: 1,
            exp: 0,
            activity_score: 80,
        };
        let reserve2 = Character {
            id: "char_reserve_priest".to_string(),
            name: "ルシアン".to_string(),
            job_id: "priest".to_string(),
            level: 12,
            base_stats: Stats::new(10, 10, 15, 35, 25, 10),
            equipments: vec![],
            learned_skills: vec![],
            gambit_rules: vec![],
            generation: 1,
            exp: 0,
            activity_score: 80,
        };
        let reserve3 = Character {
            id: "char_reserve_thief".to_string(),
            name: "クララ".to_string(),
            job_id: "thief".to_string(),
            level: 12,
            base_stats: Stats::new(25, 25, 10, 10, 25, 20),
            equipments: vec![],
            learned_skills: vec![],
            gambit_rules: vec![],
            generation: 1,
            exp: 0,
            activity_score: 80,
        };
        roster.add_member(reserve1);
        roster.add_member(reserve2);
        roster.add_member(reserve3);

        // 初期倉庫アイテム
        let mut storage_items = HashMap::new();
        storage_items.insert("red_potion".to_string(), 50);
        storage_items.insert("white_potion".to_string(), 20);
        storage_items.insert("oridecon".to_string(), 15);
        storage_items.insert("iron_ore".to_string(), 40);
        storage_items.insert("coal".to_string(), 20);
        storage_items.insert("card_andre".to_string(), 4);
        storage_items.insert("card_doppelganger".to_string(), 1);
        storage_items.insert("card_sidewinder".to_string(), 2);

        // 初期倉庫装備（ベース武器等）
        let storage_equipments = vec![
            Equipment {
                id: "damascus_01".to_string(),
                name: "ダマスカス".to_string(),
                slot: EquipSlot::Weapon,
                weapon_atk: 118,
                refine: 0,
                slots: 2,
                cards: vec![],
                modifiers: vec![],
                is_locked: false,
            },
            Equipment {
                id: "damascus_02".to_string(),
                name: "ダマスカス".to_string(),
                slot: EquipSlot::Weapon,
                weapon_atk: 118,
                refine: 0,
                slots: 2,
                cards: vec![],
                modifiers: vec![],
                is_locked: false,
            },
            Equipment {
                id: "damascus_03".to_string(),
                name: "ダマスカス".to_string(),
                slot: EquipSlot::Weapon,
                weapon_atk: 118,
                refine: 0,
                slots: 2,
                cards: vec![],
                modifiers: vec![],
                is_locked: false,
            },
            Equipment {
                id: "chain_mail_01".to_string(),
                name: "チェーンメイル".to_string(),
                slot: EquipSlot::Armor,
                weapon_atk: 0,
                refine: 0,
                slots: 1,
                cards: vec![],
                modifiers: vec![],
                is_locked: false,
            },
        ];

        // 3大ライバルギルドの結成
        let mut rival_guilds = Vec::new();

        // 1. 『黒曜の旅団』（Obsidian Brigade: 物理速攻＆重装甲）
        let mut obsidian = RivalGuild::new("黒曜の旅団");
        obsidian.roster.push(Character {
            id: "rival_obsidian_1".to_string(),
            name: "首領ダミアン".to_string(),
            job_id: "assassin".to_string(),
            level: 45,
            base_stats: Stats::new(75, 80, 40, 10, 50, 30),
            equipments: vec![Equipment {
                id: "obs_dagger".to_string(),
                name: "+8クリシュナ".to_string(),
                slot: EquipSlot::Weapon,
                weapon_atk: 140,
                refine: 8,
                slots: 2,
                cards: vec![],
                modifiers: vec![],
                is_locked: true,
            }],
            learned_skills: vec![],
            gambit_rules: vec![],
            generation: 1,
            exp: 0,
            activity_score: 100,
        });
        obsidian.roster.push(Character {
            id: "rival_obsidian_2".to_string(),
            name: "重斧ゴルドー".to_string(),
            job_id: "blacksmith".to_string(),
            level: 46,
            base_stats: Stats::new(60, 40, 50, 10, 70, 40),
            equipments: vec![],
            learned_skills: vec![],
            gambit_rules: vec![],
            generation: 1,
            exp: 0,
            activity_score: 100,
        });
        obsidian.roster.push(Character {
            id: "rival_obsidian_3".to_string(),
            name: "鋼壁レオン".to_string(),
            job_id: "knight".to_string(),
            level: 44,
            base_stats: Stats::new(50, 30, 80, 10, 40, 20),
            equipments: vec![],
            learned_skills: vec![],
            gambit_rules: vec![],
            generation: 1,
            exp: 0,
            activity_score: 100,
        });
        rival_guilds.push(obsidian);

        // 2. 『鉄棺騎士団』（Iron Coffin Knights: 防御・反射特化）
        let mut iron_coffin = RivalGuild::new("鉄棺騎士団");
        iron_coffin.roster.push(Character {
            id: "rival_iron_1".to_string(),
            name: "聖騎士アイゼン".to_string(),
            job_id: "crusader".to_string(),
            level: 47,
            base_stats: Stats::new(40, 20, 95, 20, 40, 25),
            equipments: vec![Equipment {
                id: "iron_shield".to_string(),
                name: "+9反射のバックラー".to_string(),
                slot: EquipSlot::Shield,
                weapon_atk: 10,
                refine: 9,
                slots: 1,
                cards: vec![],
                modifiers: vec![],
                is_locked: true,
            }],
            learned_skills: vec![],
            gambit_rules: vec![],
            generation: 1,
            exp: 0,
            activity_score: 100,
        });
        iron_coffin.roster.push(Character {
            id: "rival_iron_2".to_string(),
            name: "重歩兵ジーク".to_string(),
            job_id: "knight".to_string(),
            level: 43,
            base_stats: Stats::new(45, 25, 85, 10, 35, 20),
            equipments: vec![],
            learned_skills: vec![],
            gambit_rules: vec![],
            generation: 1,
            exp: 0,
            activity_score: 100,
        });
        iron_coffin.roster.push(Character {
            id: "rival_iron_3".to_string(),
            name: "修道女ヒルダ".to_string(),
            job_id: "priest".to_string(),
            level: 45,
            base_stats: Stats::new(20, 20, 60, 60, 50, 30),
            equipments: vec![],
            learned_skills: vec![],
            gambit_rules: vec![],
            generation: 1,
            exp: 0,
            activity_score: 100,
        });
        rival_guilds.push(iron_coffin);

        // 3. 『灰燼の学派』（Ash Scholars: 魔法・支援特化）
        let mut ash_scholars = RivalGuild::new("灰燼の学派");
        ash_scholars.roster.push(Character {
            id: "rival_ash_1".to_string(),
            name: "大魔導アルス".to_string(),
            job_id: "wizard".to_string(),
            level: 46,
            base_stats: Stats::new(10, 20, 35, 95, 65, 20),
            equipments: vec![Equipment {
                id: "ash_staff".to_string(),
                name: "+8知力の大杖".to_string(),
                slot: EquipSlot::Weapon,
                weapon_atk: 90,
                refine: 8,
                slots: 2,
                cards: vec![],
                modifiers: vec![],
                is_locked: true,
            }],
            learned_skills: vec![],
            gambit_rules: vec![],
            generation: 1,
            exp: 0,
            activity_score: 100,
        });
        ash_scholars.roster.push(Character {
            id: "rival_ash_2".to_string(),
            name: "付術師セレーナ".to_string(),
            job_id: "sage".to_string(),
            level: 44,
            base_stats: Stats::new(20, 40, 40, 80, 50, 20),
            equipments: vec![],
            learned_skills: vec![],
            gambit_rules: vec![],
            generation: 1,
            exp: 0,
            activity_score: 100,
        });
        ash_scholars.roster.push(Character {
            id: "rival_ash_3".to_string(),
            name: "詠唱士カレン".to_string(),
            job_id: "priest".to_string(),
            level: 42,
            base_stats: Stats::new(15, 20, 45, 70, 55, 30),
            equipments: vec![],
            learned_skills: vec![],
            gambit_rules: vec![],
            generation: 1,
            exp: 0,
            activity_score: 100,
        });
        rival_guilds.push(ash_scholars);

        // 市場の初期化（ModDatabase定義からロード）
        let market = MarketSystem::from_database(db);

        Self {
            generation: 1,
            calendar_tick: 0,
            treasury_zeny: 50_000,
            storage_equipments,
            storage_items,
            active_character: active_char,
            active_growth_plan: GrowthPlan::AgiCrit,
            party_members: vec![vargas, otto],
            roster,
            rival_guilds,
            market,
            active_dispatch: None,
            rival_league_logs: Vec::new(),
        }
    }

    /// ワールド全体のTick進行連動処理
    /// プレイヤーが1回の冒険や活動を行うごとに裏で派遣・市場・ライバルリーグ戦が同期進行する
    pub fn advance_turn(&mut self, elapsed_ticks: u32, db: &ModDatabase) -> Vec<String> {
        let mut turn_logs = Vec::new();
        self.calendar_tick += elapsed_ticks;

        turn_logs.push(format!(
            "⏱️ [ワールド進行] 時が {} Tick 経過しました（累計: {} Tick）",
            elapsed_ticks, self.calendar_tick
        ));

        // 1. 自動派遣部隊の稼働消化と回収
        if let Some(mut dispatch) = self.active_dispatch.take() {
            if dispatch.ticks_remaining <= elapsed_ticks {
                // 派遣完了！
                if let Some(area) = db.areas.get(&dispatch.area_id) {
                    let mut dummy_potions = self.storage_items.get("white_potion").copied().unwrap_or(20);
                    let result = DispatchSystem::dispatch_to_area(
                        &dispatch.members,
                        area,
                        dispatch.cycles,
                        &mut dummy_potions,
                    );
                    self.storage_items.insert("white_potion".to_string(), dummy_potions);

                    turn_logs.push(format!(
                        " 📦 【派遣帰還】『{}』へ派遣されていた部隊が帰還しました！ (獲得Zeny: +{} Zeny)",
                        area.name, result.zeny_earned
                    ));
                    self.treasury_zeny += result.zeny_earned;

                    // ドロップアイテムの倉庫格納
                    for (item_id, count) in &result.gathered_items {
                        let cur = self.storage_items.entry(item_id.clone()).or_insert(0);
                        *cur += count;
                        turn_logs.push(format!("   - 獲得物資: {} x{}", item_id, count));
                    }

                    // 経験値付与と名簿への復帰
                    for mut member in dispatch.members {
                        member.activity_score = (member.activity_score + 15).min(100);
                        let exp_gain = (area.recommended_level as u64) * 800;
                        let _ = Progression::grant_exp(&mut member, exp_gain, GrowthPlan::DexLukCrafter, &mut turn_logs);
                        self.roster.add_member(member);
                    }
                }
            } else {
                dispatch.ticks_remaining -= elapsed_ticks;
                turn_logs.push(format!(
                    " ⛏️ [遠征中] 『{}』への自動派遣が進行中（残り {} Tick）",
                    dispatch.area_id, dispatch.ticks_remaining
                ));
                self.active_dispatch = Some(dispatch);
            }
        }

        // 2. 動的市場の自然回復とBOM自動補充
        self.market.natural_recovery();
        turn_logs.push(" 📈 [市場動向] 商人ギルドの流通により市場在庫が自然回復・BOM補充されました。".to_string());

        // 3. ライバルギルド群の裏リーグ戦（2ギルドを選出して模擬戦と成長）
        if self.rival_guilds.len() >= 2 {
            let mut rng = rand::thread_rng();
            let idx1 = rng.gen_range(0..self.rival_guilds.len());
            let mut idx2 = rng.gen_range(0..self.rival_guilds.len());
            while idx1 == idx2 {
                idx2 = rng.gen_range(0..self.rival_guilds.len());
            }

            let g1_name = self.rival_guilds[idx1].name.clone();
            let g2_name = self.rival_guilds[idx2].name.clone();

            // 簡易勝敗判定（平均レベル＋乱数）
            let g1_avg_lv: f32 = self.rival_guilds[idx1].roster.iter().map(|c| c.level as f32).sum::<f32>()
                / (self.rival_guilds[idx1].roster.len().max(1) as f32);
            let g2_avg_lv: f32 = self.rival_guilds[idx2].roster.iter().map(|c| c.level as f32).sum::<f32>()
                / (self.rival_guilds[idx2].roster.len().max(1) as f32);

            let g1_score = g1_avg_lv + rng.gen_range(0.0..10.0);
            let g2_score = g2_avg_lv + rng.gen_range(0.0..10.0);

            let (winner_idx, loser_name) = if g1_score >= g2_score {
                (idx1, g2_name)
            } else {
                (idx2, g1_name)
            };

            let winner = &mut self.rival_guilds[winner_idx];
            // 勝者の成長（微量のレベルアップ抽選）
            if let Some(mvp) = winner.roster.first_mut() {
                if mvp.level < 80 && rng.gen_bool(0.4) {
                    mvp.level += 1;
                    mvp.base_stats.add(StatType::Str, 2);
                    mvp.base_stats.add(StatType::Vit, 2);
                }
            }

            let league_log = format!(
                " ⚔️ [ギルドリーグ裏対決] 『{}』が『{}』との模擬戦を制しました！（勝者戦力微増）",
                winner.name, loser_name
            );
            turn_logs.push(league_log.clone());
            self.rival_league_logs.push(league_log);
            if self.rival_league_logs.len() > 10 {
                self.rival_league_logs.remove(0);
            }
        }

        turn_logs
    }

    /// ワールドステートをRON形式でファイルに保存
    pub fn save_to_file(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        let pretty_cfg = ron::ser::PrettyConfig::default();
        let ron_str = ron::ser::to_string_pretty(self, pretty_cfg)?;
        std::fs::write(path, ron_str)?;
        Ok(())
    }

    /// RON形式のファイルからワールドステートを復元
    pub fn load_from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let state: WorldState = ron::from_str(&content)?;
        Ok(state)
    }

    /// 現在の出撃パーティー（現役キャラ＋同行メンバー、最大3名）を取得
    pub fn get_active_party(&self) -> Vec<Character> {
        let mut pt = Vec::new();
        pt.push(self.active_character.clone());
        for m in self.party_members.iter().take(2) {
            pt.push(m.clone());
        }
        pt
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::ModDatabase;

    #[test]
    fn test_world_state_save_and_load() {
        let db = ModDatabase::load_from_dir("mods/core").expect("Failed to load core mods");
        let mut world = WorldState::new_game(&db);

        // 状態を少し変更
        world.treasury_zeny = 88_888;
        world.calendar_tick = 350;
        world.generation = 2;
        world.storage_items.insert("special_test_gem".to_string(), 7);

        let test_save_path = "saves/test_save.ron";
        world.save_to_file(test_save_path).expect("Failed to save world state");

        let loaded = WorldState::load_from_file(test_save_path).expect("Failed to load world state");

        // クリーンアップ
        let _ = std::fs::remove_file(test_save_path);

        assert_eq!(loaded.treasury_zeny, 88_888);
        assert_eq!(loaded.calendar_tick, 350);
        assert_eq!(loaded.generation, 2);
        assert_eq!(loaded.storage_items.get("special_test_gem"), Some(&7));
        assert_eq!(loaded.active_character.name, "クロード");
        assert_eq!(loaded.rival_guilds.len(), 3);
    }

    #[test]
    fn test_world_advance_turn_sync() {
        let db = ModDatabase::load_from_dir("mods/core").expect("Failed to load core mods");
        let mut world = WorldState::new_game(&db);

        assert_eq!(world.calendar_tick, 0);
        let logs = world.advance_turn(100, &db);

        assert_eq!(world.calendar_tick, 100);
        assert!(!logs.is_empty());
        assert!(!world.rival_league_logs.is_empty());
    }
}

