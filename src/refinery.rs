use rand::Rng;
use crate::models::{Character, Equipment, StatType};

/// 精錬の基本コスト（1回あたりのZeny）
#[allow(dead_code)]
pub const REFINE_BASE_COST: u32 = 300;

/// 精錬の安全圏上限（100%成功）
#[allow(dead_code)]
pub const SAFE_REFINE_LIMIT: u8 = 4;

/// ワークオーダー（バッチ精錬指示）の実行モード
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum WorkOrderMode {
    /// 安全圏内で指定レベルまで精錬（破壊リスクなし）
    SafeLimit(u8),
    /// 予算と予備武器が許す限り目標値を目指して叩き続ける（クホホリスクあり）
    BudgetBurn {
        target_level: u8,
        max_zeny: u32,
        max_weapons: usize,
    },
}

/// ワークオーダー（作業指示書）
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct WorkOrder {
    pub order_name: String,
    pub mode: WorkOrderMode,
    pub target_level: u8,
}

/// ワークオーダーのバッチ実行結果
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct WorkOrderReport {
    pub completed_weapons: Vec<Equipment>,
    pub broken_count: usize,
    pub total_zeny_spent: u32,
    pub logs: Vec<String>,
}

/// 精錬システム（クホホ工場）
#[allow(dead_code)]
pub struct Refinery;

#[allow(dead_code)]
impl Refinery {
    /// 基礎成功率テーブル（0 -> 1 は +1 チャレンジ）
    /// レベルに応じた基本成功率（%）
    pub fn get_base_success_rate(current_level: u8) -> f32 {
        match current_level {
            0..=3 => 100.0, // +1〜+4は100%安全圏
            4 => 70.0,      // +5挑戦
            5 => 55.0,      // +6挑戦
            6 => 40.0,      // +7挑戦
            7 => 30.0,      // +8挑戦
            8 => 20.0,      // +9挑戦
            9 => 15.0,      // +10挑戦
            10 => 10.0,     // +11挑戦
            11 => 7.0,      // +12挑戦
            12 => 5.0,      // +13挑戦
            13 => 3.0,      // +14挑戦
            14 => 2.0,      // +15挑戦
            _ => 1.0,
        }
    }

    /// 担当鍛冶屋のステータス（DEX・LUK）による成功率ボーナス（最大+15%）
    /// Why: 非戦闘の製造職（DEX/LUK極）に特化するプレイヤーの努力を称え、
    /// 過剰精錬の成功率を底上げする強力な職能シナジーを表現するため。
    pub fn calculate_smith_bonus(smith: &Character) -> f32 {
        let dex = smith.base_stats.get(StatType::Dex) as f32;
        let luk = smith.base_stats.get(StatType::Luk) as f32;
        let bonus = (dex + luk) / 12.0;
        bonus.min(15.0) // 上限15%
    }

    /// 1本の武器に対して1回の精錬を実行する
    /// 戻り値: Result<Option<Equipment>, String>
    /// - Ok(Some(weapon)): 精錬成功、レベル+1
    /// - Ok(None): 精錬失敗（クホホ……ロスト）
    /// - Err(reason): 安全装置等による拒否
    pub fn refine_once(
        weapon: &mut Equipment,
        smith: Option<&Character>,
        logs: &mut Vec<String>,
    ) -> Result<bool, String> {
        let current_lvl = weapon.refine;
        if current_lvl >= 15 {
            return Err("これ以上の精錬はできません（最大+15）".to_string());
        }

        // 破壊リスクの判定
        let is_unsafe = current_lvl >= SAFE_REFINE_LIMIT;

        // 安全装置（フェイルセーフ）：希少品ロックがかかっている場合は破壊リスク精錬を拒否
        if is_unsafe && weapon.is_locked {
            let msg = format!(
                "🛑 [安全装置作動] 「{}」は希少品ロック中のため、破壊リスクのある精錬(+{})を中止しました。",
                weapon.name, current_lvl + 1
            );
            logs.push(msg.clone());
            return Err(msg);
        }

        let base_rate = Self::get_base_success_rate(current_lvl);
        let smith_bonus = smith.map(Self::calculate_smith_bonus).unwrap_or(0.0);
        let final_rate = (base_rate + smith_bonus).min(100.0);

        let mut rng = rand::thread_rng();
        let roll: f32 = rng.gen_range(0.0..100.0);

        let smith_name = smith.map(|s| s.name.as_str()).unwrap_or("一般NPC鍛冶屋");

        if roll < final_rate {
            // 精錬成功！
            weapon.refine += 1;
            // 武器名の更新表示用プレフィックス
            let refine_tag = format!("+{}", weapon.refine);
            let breakthrough_tag = if weapon.refine == 7 {
                " ★【限界突破 +7達成: クリティカル率&ASPD向上解放!】"
            } else if weapon.refine == 10 {
                " 🌟★【神話限界突破 +10達成: 防御無視追加ダメージ&オートスペル発動率2倍解放!!】"
            } else {
                ""
            };

            logs.push(format!(
                "   [精錬成功] (担当: {}) {} -> {}! (成功率: {:.1}%){}",
                smith_name, current_lvl, refine_tag, final_rate, breakthrough_tag
            ));
            Ok(true)
        } else {
            // 精錬失敗……クホホ！
            logs.push(format!(
                "   [精錬失敗] クホホ……「+{} {}」は激しい槌音と共に砕け散りました。(成功率: {:.1}%)",
                current_lvl, weapon.name, final_rate
            ));
            Ok(false)
        }
    }

    /// ワークオーダー（バッチ指示）に基づき、複数のベース武器を自動精錬する
    pub fn process_work_order(
        order: &WorkOrder,
        mut weapons: Vec<Equipment>,
        smith: &Character,
        mut current_zeny: u32,
    ) -> (WorkOrderReport, u32) {
        let mut logs = Vec::new();
        let mut completed = Vec::new();
        let mut broken_count = 0;
        let mut total_zeny_spent = 0;

        let smith_bonus = Self::calculate_smith_bonus(smith);
        logs.push(format!(
            "🔨 ワークオーダー開始: 『{}』 (目標: +{})",
            order.order_name, order.target_level
        ));
        logs.push(format!(
            "   担当鍛冶屋: {} (DEX:{}, LUK:{}) -> 職能精錬ボーナス: +{:.1}%",
            smith.name, smith.base_stats.dex, smith.base_stats.luk, smith_bonus
        ));
        logs.push(format!(
            "   投入ベース武器: {}本 | 所持資金: {} Zeny",
            weapons.len(), current_zeny
        ));

        match order.mode {
            WorkOrderMode::SafeLimit(limit) => {
                let target = limit.min(SAFE_REFINE_LIMIT);
                for mut w in weapons {
                    while w.refine < target && current_zeny >= REFINE_BASE_COST {
                        current_zeny -= REFINE_BASE_COST;
                        total_zeny_spent += REFINE_BASE_COST;
                        let _ = Self::refine_once(&mut w, Some(smith), &mut logs);
                    }
                    completed.push(w);
                }
            }
            WorkOrderMode::BudgetBurn { target_level, max_zeny, max_weapons } => {
                let process_count = weapons.len().min(max_weapons);
                let mut queue: Vec<Equipment> = weapons.drain(0..process_count).collect();
                let budget_limit = total_zeny_spent + max_zeny;

                let mut weapon_idx = 0;
                let mut best_kept_refine = 0u8;

                while !queue.is_empty() && current_zeny >= REFINE_BASE_COST && total_zeny_spent < budget_limit {
                    weapon_idx += 1;
                    let mut weapon = queue.remove(0);
                    let remaining_in_queue = queue.len();

                    logs.push(format!(
                        " ▶ [試行 #{}] 「+{} {}」の精錬開始 (残り予備: {}本)...",
                        weapon_idx, weapon.refine, weapon.name, remaining_in_queue
                    ));

                    let mut is_broken = false;
                    while weapon.refine < target_level
                        && current_zeny >= REFINE_BASE_COST
                        && total_zeny_spent < budget_limit
                    {
                        // 職人の知恵（セーフティ）:
                        // 予備が残り少なくなってきた場合、または既に+9（準最高峰）に達し、
                        // かつ手持ちにそれ以上の武器がない場合は、全滅を防ぐためキープする！
                        if weapon.refine >= 8 && remaining_in_queue == 0 && best_kept_refine < weapon.refine {
                            logs.push(format!(
                                " 💡 [職人の判断] 予備が尽きたため、現最高峰の「+{} {}」をロストさせずに確保します！",
                                weapon.refine, weapon.name
                            ));
                            completed.push(weapon.clone());
                            break;
                        }

                        current_zeny -= REFINE_BASE_COST;
                        total_zeny_spent += REFINE_BASE_COST;

                        match Self::refine_once(&mut weapon, Some(smith), &mut logs) {
                            Ok(true) => {
                                if weapon.refine == target_level {
                                    logs.push(format!(
                                        " ✨ 神業達成！至高の目標「+{} {}」が完成しました！",
                                        weapon.refine, weapon.name
                                    ));
                                    completed.push(weapon.clone());
                                    best_kept_refine = best_kept_refine.max(weapon.refine);
                                    break;
                                }
                            }
                            Ok(false) => {
                                broken_count += 1;
                                is_broken = true;
                                break;
                            }
                            Err(_) => {
                                completed.push(weapon.clone());
                                break;
                            }
                        }
                    }

                    if !is_broken && weapon.refine < target_level && weapon.refine >= SAFE_REFINE_LIMIT {
                        completed.push(weapon);
                    }
                }
            }
        }

        // 精錬値の降順でソート
        completed.sort_by(|a, b| b.refine.cmp(&a.refine));

        logs.push(format!(
            "\n📋 ワークオーダー完了報告: 生存納品: {}本 / 破損消滅: {}本 / 消費Zeny: {} Zeny (残高: {} Zeny)",
            completed.len(), broken_count, total_zeny_spent, current_zeny
        ));

        let report = WorkOrderReport {
            completed_weapons: completed,
            broken_count,
            total_zeny_spent,
            logs,
        };

        (report, current_zeny)
    }

    /// 過剰精錬による限界突破ボーナスを判定・取得
    pub fn get_breakthrough_bonus(refine_level: u8) -> (f32, f32, u32, f32) {
        // (aspd_bonus_percent, crit_bonus, ignore_def_damage, autospell_chance_mult)
        let mut aspd_bonus = 0.0;
        let mut crit_bonus = 0.0;
        let mut ignore_def_dmg = 0;
        let mut autospell_mult = 1.0;

        if refine_level >= 7 {
            // +7以上: クリティカル率 +5%, ASPD +4%
            crit_bonus += 5.0;
            aspd_bonus += 4.0;
        }

        if refine_level >= 10 {
            // +10以上: 防御無視ダメージ +120, オートスペル確率2.0倍
            ignore_def_dmg += 120;
            autospell_mult = 2.0;
        }

        (aspd_bonus, crit_bonus, ignore_def_dmg, autospell_mult)
    }
}
