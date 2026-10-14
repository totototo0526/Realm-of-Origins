use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::models::{AreaDef, Character};

/// 自動派遣の目的地（ファーミング先）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DispatchDestination {
    /// 廃鉱山（鉄鉱石、石炭、オリデオコンなどの鉱物素材）
    AbandonedMine,
    /// 薬草の湿原（白ハーブ、空瓶などの薬品素材）
    HerbForest,
    /// 外部エリア定義ID
    Custom(String),
}

impl DispatchDestination {
    pub fn name_ja(&self) -> &'static str {
        match self {
            Self::AbandonedMine => "廃坑の深層（鉱山資源採掘）",
            Self::HerbForest => "妖精の薬草園（白ハーブ採集）",
            Self::Custom(_) => "外部開拓エリア",
        }
    }
}

/// 派遣部隊のシナジー効果
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DispatchSynergy {
    pub active_synergies: Vec<String>,
    pub yield_multiplier: f64,      // 素材獲得量倍率（基本1.0）
    pub cost_reduction_rate: f64,   // 消耗品削減率（0.0〜1.0）
    pub extra_rare_chance: f64,     // レア素材追加率
}

/// 進行中の自動派遣ミッション（ワールドステートで管理）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveDispatch {
    pub area_id: String,
    pub area_name: String,
    pub members: Vec<Character>,
    pub ticks_remaining: u32,
    pub cycles: u32,
}

/// 自動派遣システム
pub struct DispatchSystem;

/// 派遣1回あたりの成果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct DispatchResult {
    pub destination: DispatchDestination,
    pub cycles: u32,
    pub gathered_items: HashMap<String, u32>,
    pub zeny_earned: u32,
    pub white_potions_consumed: u32,
    pub synergy: DispatchSynergy,
    pub logs: Vec<String>,
}

impl DispatchSystem {
    /// 部隊メンバーのジョブ・特性からシナジーを計算する
    pub fn calculate_synergy(members: &[Character]) -> DispatchSynergy {
        let mut has_merchant = false;
        let mut has_support = false;
        let mut has_striker = false;

        for m in members {
            match m.job_id.as_str() {
                "merchant" | "blacksmith" => has_merchant = true,
                "acolyte" | "priest" => has_support = true,
                "thief" | "assassin" | "knight" | "swordsman" => has_striker = true,
                _ => {}
            }
        }

        let mut synergy = DispatchSynergy {
            active_synergies: Vec::new(),
            yield_multiplier: 1.0,
            cost_reduction_rate: 0.0,
            extra_rare_chance: 0.0,
        };

        if has_merchant {
            synergy.active_synergies.push("【大容量カート】素材獲得量 +30%".to_string());
            synergy.yield_multiplier += 0.3;
        }
        if has_support {
            synergy.active_synergies.push("【セルフヒール】ポーション消費 -40%".to_string());
            synergy.cost_reduction_rate += 0.4;
        }
        if has_striker {
            synergy.active_synergies.push("【護衛殲滅】危険排除・レア採掘率 +25%".to_string());
            synergy.extra_rare_chance += 0.25;
        }

        // 3職業が揃った黄金トリオボーナス
        if has_merchant && has_support && has_striker {
            synergy.active_synergies.push("🌟【完全自給自足トリオ】素材獲得 +50% & 消耗品消費 -50%".to_string());
            synergy.yield_multiplier += 0.2; // 合計 +50%
            synergy.cost_reduction_rate = 0.5; // 50%カット
        }

        synergy
    }

    /// 外部エリア定義（AreaDef）に基づき、控え部隊を派遣して裏作業を実行
    pub fn dispatch_to_area(
        members: &[Character],
        area: &AreaDef,
        cycles: u32,
        available_potions: &mut u32,
    ) -> DispatchResult {
        let synergy = Self::calculate_synergy(members);
        let mut gathered_items: HashMap<String, u32> = HashMap::new();
        let mut total_zeny: u32 = 0;
        let mut total_potions_consumed: u32 = 0;
        let mut logs = Vec::new();

        logs.push(format!(
            "🚩 控え部隊（{}名）を 『{}』 へ {}サイクル 派遣開始！",
            members.len(),
            area.name,
            cycles
        ));

        for syn in &synergy.active_synergies {
            logs.push(format!("   ✨ シナジー発動: {}", syn));
        }

        // エリアのスタミナ消費コスト（デフォルトは白ポーション2本）
        let base_potion_cost = area.stamina_cost_item.as_ref().map(|(_, c)| *c as f64).unwrap_or(2.0);
        let effective_potion_cost = (base_potion_cost * (1.0 - synergy.cost_reduction_rate)).round().max(1.0) as u32;

        for cycle in 1..=cycles {
            if *available_potions < effective_potion_cost {
                logs.push(format!(
                    "   ⚠️ [Cycle {}] 白ポーションが枯渇したため、派遣を途中中断しました（残り0本）。",
                    cycle
                ));
                break;
            }

            *available_potions -= effective_potion_cost;
            total_potions_consumed += effective_potion_cost;

            // ドロップテーブルに基づいて素材を獲得
            for drop_def in &area.dispatch_drops {
                if drop_def.is_rare {
                    // レア素材: 奇数サイクルまたはレア率ボーナスで獲得
                    if cycle % 2 == 0 || synergy.extra_rare_chance > 0.0 {
                        let count = ((drop_def.base_count as f64 * synergy.yield_multiplier).round()).max(1.0) as u32;
                        *gathered_items.entry(drop_def.item_id.clone()).or_insert(0) += count;
                    }
                } else {
                    let count = ((drop_def.base_count as f64 * synergy.yield_multiplier).round()).max(1.0) as u32;
                    *gathered_items.entry(drop_def.item_id.clone()).or_insert(0) += count;
                }
            }

            total_zeny += area.base_zeny_reward;
        }

        logs.push(format!(
            "✅ 派遣任務完了: 白ポーション {}本 消費 | 探索Zeny {} 獲得",
            total_potions_consumed, total_zeny
        ));

        DispatchResult {
            destination: DispatchDestination::Custom(area.id.clone()),
            cycles,
            gathered_items,
            zeny_earned: total_zeny,
            white_potions_consumed: total_potions_consumed,
            synergy,
            logs,
        }
    }

    /// 従来のDispatchDestinationに基づく派遣（後方互換用）
    pub fn dispatch(
        members: &[Character],
        destination: DispatchDestination,
        cycles: u32,
        available_potions: &mut u32,
    ) -> DispatchResult {
        let (area_id, area_name, drops, base_zeny) = match destination {
            DispatchDestination::AbandonedMine => (
                "abandoned_mine",
                "廃坑の深層（鉱山資源採掘）",
                vec![
                    crate::models::DispatchDropDef { item_id: "iron_ore".to_string(), base_count: 10, is_rare: false },
                    crate::models::DispatchDropDef { item_id: "coal".to_string(), base_count: 5, is_rare: false },
                    crate::models::DispatchDropDef { item_id: "oridecon".to_string(), base_count: 1, is_rare: true },
                ],
                500,
            ),
            DispatchDestination::HerbForest => (
                "herb_forest",
                "妖精の薬草園（白ハーブ採集）",
                vec![
                    crate::models::DispatchDropDef { item_id: "white_herb".to_string(), base_count: 15, is_rare: false },
                    crate::models::DispatchDropDef { item_id: "empty_bottle".to_string(), base_count: 10, is_rare: false },
                ],
                400,
            ),
            DispatchDestination::Custom(ref id) => (
                id.as_str(),
                "未知の開拓地",
                vec![],
                300,
            ),
        };

        let temp_area = AreaDef {
            id: area_id.to_string(),
            name: area_name.to_string(),
            recommended_level: 20,
            stamina_cost_item: Some(("white_potion".to_string(), 2)),
            monsters: vec![],
            dispatch_drops: drops,
            base_zeny_reward: base_zeny,
        };

        let mut res = Self::dispatch_to_area(members, &temp_area, cycles, available_potions);
        res.destination = destination;
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Stats;

    #[test]
    fn test_dispatch_synergy_and_farming() {
        let merchant = Character::new_with_gen("m1", "商人", "merchant", 1, 1, Stats::default());
        let support = Character::new_with_gen("s1", "支援", "priest", 1, 1, Stats::default());
        let striker = Character::new_with_gen("k1", "剣士", "knight", 1, 1, Stats::default());

        let party = vec![merchant, support, striker];
        let synergy = DispatchSystem::calculate_synergy(&party);

        // 黄金トリオボーナス: 素材+50%, 消費-50%
        assert!(synergy.yield_multiplier >= 1.5);
        assert!(synergy.cost_reduction_rate >= 0.5);

        let mut potions = 10u32;
        let res = DispatchSystem::dispatch(&party, DispatchDestination::AbandonedMine, 5, &mut potions);

        assert_eq!(res.white_potions_consumed, 5); // 1サイクル1本消費
        assert_eq!(potions, 5);
        assert!(*res.gathered_items.get("iron_ore").unwrap() >= 50);
    }
}
