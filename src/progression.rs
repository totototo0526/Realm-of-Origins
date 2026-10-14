use serde::{Deserialize, Serialize};
use crate::models::{Character, StatType, Stats};

/// ビルド育成方針（レベルアップ時のステータス自動割り振りテンプレート）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GrowthPlan {
    /// 高速アタッカー方針: AGI 40%, STR 30%, LUK 20%, DEX 10%
    AgiCrit,
    /// 強靭タンク方針: VIT 50%, STR 25%, DEX 15%, AGI 10%
    VitTank,
    /// 職人・商才方針: DEX 45%, LUK 45%, VIT 10%
    DexLukCrafter,
    /// 魔法・支援方針: INT 50%, DEX 30%, VIT 20%
    IntDexSupport,
}

impl GrowthPlan {
    pub fn name_ja(&self) -> &'static str {
        match self {
            GrowthPlan::AgiCrit => "AGI/STR速攻型",
            GrowthPlan::VitTank => "VIT重装壁型",
            GrowthPlan::DexLukCrafter => "DEX/LUK製造商才型",
            GrowthPlan::IntDexSupport => "INT/DEX魔導支援型",
        }
    }

    /// ステータスポイントをプランの比率に従って配分
    pub fn allocate_points(&self, stats: &mut Stats, points: u16) {
        match self {
            GrowthPlan::AgiCrit => {
                let agi = (points as f32 * 0.40).round() as u16;
                let str_p = (points as f32 * 0.30).round() as u16;
                let luk = (points as f32 * 0.20).round() as u16;
                let dex = points.saturating_sub(agi + str_p + luk);
                stats.add(StatType::Agi, agi as i32);
                stats.add(StatType::Str, str_p as i32);
                stats.add(StatType::Luk, luk as i32);
                stats.add(StatType::Dex, dex as i32);
            }
            GrowthPlan::VitTank => {
                let vit = (points as f32 * 0.50).round() as u16;
                let str_p = (points as f32 * 0.25).round() as u16;
                let dex = (points as f32 * 0.15).round() as u16;
                let agi = points.saturating_sub(vit + str_p + dex);
                stats.add(StatType::Vit, vit as i32);
                stats.add(StatType::Str, str_p as i32);
                stats.add(StatType::Dex, dex as i32);
                stats.add(StatType::Agi, agi as i32);
            }
            GrowthPlan::DexLukCrafter => {
                let dex = (points as f32 * 0.45).round() as u16;
                let luk = (points as f32 * 0.45).round() as u16;
                let vit = points.saturating_sub(dex + luk);
                stats.add(StatType::Dex, dex as i32);
                stats.add(StatType::Luk, luk as i32);
                stats.add(StatType::Vit, vit as i32);
            }
            GrowthPlan::IntDexSupport => {
                let int_p = (points as f32 * 0.50).round() as u16;
                let dex = (points as f32 * 0.30).round() as u16;
                let vit = points.saturating_sub(int_p + dex);
                stats.add(StatType::Int, int_p as i32);
                stats.add(StatType::Dex, dex as i32);
                stats.add(StatType::Vit, vit as i32);
            }
        }
    }
}

/// レベルごとの累計必要経験値テーブル
pub fn required_exp_for_level(level: u32) -> u64 {
    if level <= 1 {
        return 0;
    }
    // Lv1->2: 100, Lv2->3: 250, Lv5: 1200, Lv10: 4500, Lv20: 18000, Lv30: 55000
    let l = (level - 1) as f64;
    (80.0 * l.powf(1.6) + 30.0 * l) as u64
}

/// レベルアップの管理と成長処理
pub struct Progression;

impl Progression {
    /// 経験値の付与とレベルアップ処理
    /// 戻り値: レベルアップした回数
    pub fn grant_exp(character: &mut Character, exp_gain: u64, plan: GrowthPlan, logs: &mut Vec<String>) -> u32 {
        character.exp += exp_gain;
        let mut level_ups = 0;

        loop {
            let next_req = required_exp_for_level(character.level + 1);
            if character.exp >= next_req && character.level < 99 {
                character.level += 1;
                level_ups += 1;

                // レベルアップごとにステータスポイント獲得（Lvに応じて増加、平均4〜6ポイント）
                let points = 4 + (character.level / 15) as u16;
                plan.allocate_points(&mut character.base_stats, points);
                // 手動育成用に未割り振りポイントプールにも加算
                character.unallocated_pts += points as u32;

                logs.push(format!(
                    "   🎉 【LEVEL UP!】{} が Lv.{} に到達！ (+{} ステータスP獲得/【{}】方針適用)",
                    character.name, character.level, points, plan.name_ja()
                ));
            } else {
                break;
            }
        }

        level_ups
    }

    /// 現在のレベルと世代に応じた総獲得ステータスポイントを計算
    /// Why: [再振り] 時に全額返還プールを計算するために使用
    pub fn calculate_total_earned_stat_points(level: u32, generation: u8) -> u32 {
        level.saturating_sub(1) * 6 + (generation as u32) * 10
    }

    /// レガシー転生キャラの生成
    /// Why: 世代が進むごとに先代の経験と知識が遺伝・伝承され、初期ステータスにボーナスが加算される
    pub fn create_reincarnated_character(
        id: &str,
        name: &str,
        job_id: &str,
        generation: u8,
        plan: GrowthPlan,
    ) -> Character {
        let mut base_stats = Stats::new(10, 10, 10, 10, 10, 10);

        // レガシー転生ボーナス: (generation - 1) * 8 ポイントを初期ステータスに加算
        let bonus_points = if generation > 1 {
            let pts = (generation as u16 - 1) * 8;
            plan.allocate_points(&mut base_stats, pts);
            pts as u32
        } else {
            0
        };

        Character {
            id: id.to_string(),
            name: name.to_string(),
            job_id: job_id.to_string(),
            level: 1,
            base_stats,
            equipments: vec![],
            learned_skills: vec![],
            gambit_rules: vec![],
            generation,
            exp: 0,
            activity_score: 100,
            formation: crate::models::TacticalFormation::default(),
            unallocated_pts: bonus_points,
        }
    }
}
