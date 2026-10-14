use serde::{Deserialize, Serialize};
use crate::models::{Character, Effect, StatType, Trigger};

/// キャラクターのビルド特性タグ
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BuildTag {
    /// 物理高火力アタッカー（STR・AGI・DEX特化）
    PhysicalCarry,
    /// 武器研究・陣形支援（DEX/LUK製造職、WeaponResearch所持）
    WeaponSupport,
    /// 高耐久壁役（VIT特化、防御スキル所持）
    Tank,
    /// 魔導・追加効果（INT特化、オートスペル所持）
    MagicSupport,
    /// 超回避アタッカー（AGI特化）
    HighFlee,
}

impl BuildTag {
    pub fn name_ja(&self) -> &'static str {
        match self {
            BuildTag::PhysicalCarry => "物理キャリー",
            BuildTag::WeaponSupport => "武器支援/鍛冶屋",
            BuildTag::Tank => "重装タンク",
            BuildTag::MagicSupport => "魔導支援",
            BuildTag::HighFlee => "高速回避型",
        }
    }

    /// キャラクターのステータスとスキルから特性タグを自動判定
    pub fn analyze(ch: &Character) -> Self {
        // 1. スキルベースの判定
        let has_weapon_research = ch.learned_skills.iter().any(|s| {
            s.modifiers.iter().any(|m| {
                m.trigger == Trigger::PartyAura
                    && matches!(m.effect, Effect::PartyWeaponResearch { .. })
            })
        });
        if has_weapon_research || (ch.base_stats.get(StatType::Dex) >= 60 && ch.base_stats.get(StatType::Luk) >= 60) {
            return BuildTag::WeaponSupport;
        }

        // 2. VITとタンクスキルの判定
        let has_defense = ch.learned_skills.iter().any(|s| s.id == "iron_defense");
        if has_defense || ch.base_stats.get(StatType::Vit) >= 60 {
            return BuildTag::Tank;
        }

        // 3. INTと魔法の判定
        if ch.base_stats.get(StatType::Int) >= 40 {
            return BuildTag::MagicSupport;
        }

        // 4. AGIの判定
        if ch.base_stats.get(StatType::Agi) >= 70 {
            return BuildTag::HighFlee;
        }

        // デフォルト: 物理キャリー
        BuildTag::PhysicalCarry
    }
}

/// FA（フリーエージェント）退団候補に対する処遇選択肢
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FaDecision {
    /// ギルドに残留させる
    Retain,
    /// 街のNPCとして引退（永続パッシブボーナスを提供）
    RetireAsTownNpc,
    /// ライバルギルドへの移籍を承認（移籍金Zenyを獲得）
    ApproveTransfer,
}

/// 街のNPC化したメンバーによるギルド永続パッシブ恩恵
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TownPassiveBonus {
    pub extra_weapon_atk_percent: f32,
    pub extra_zeny_percent: f32,
    pub extra_potions_effect: f32,
}

/// ギルド名簿管理システム
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuildRoster {
    pub max_capacity: usize,
    pub active_members: Vec<Character>,
    pub retired_town_npcs: Vec<Character>,
    pub transferred_members: Vec<Character>,
    pub town_bonus: TownPassiveBonus,
    pub treasury_zeny: u32,
}

#[allow(dead_code)]
impl GuildRoster {
    pub fn new(max_capacity: usize) -> Self {
        Self {
            max_capacity,
            active_members: Vec::new(),
            retired_town_npcs: Vec::new(),
            transferred_members: Vec::new(),
            town_bonus: TownPassiveBonus::default(),
            treasury_zeny: 0,
        }
    }

    /// メンバーの追加
    pub fn add_member(&mut self, ch: Character) {
        self.active_members.push(ch);
    }

    /// 世代交代に伴う稼働スコア減算と、定員超過・低稼働時のFA判定
    /// 出撃しなかったキャラクターのスコアを減算し、活動スコア30以下または定員超過をFA候補とする
    pub fn process_generation_shift(&mut self, deployed_ids: &[String]) -> Vec<Character> {
        let mut fa_candidates = Vec::new();

        for ch in &mut self.active_members {
            if deployed_ids.contains(&ch.id) {
                ch.activity_score = (ch.activity_score + 20).min(100);
            } else {
                // 非出撃メンバーは活動スコアが大幅低下
                ch.activity_score = ch.activity_score.saturating_sub(45);
            }
        }

        // 稼働スコアの低い順にソート
        self.active_members.sort_by_key(|c| c.activity_score);

        // 定員超過または稼働スコア30以下の古参メンバーをFA退団候補に抽出
        while !self.active_members.is_empty()
            && (self.active_members.len() > self.max_capacity || self.active_members[0].activity_score <= 30)
        {
            fa_candidates.push(self.active_members.remove(0));
        }

        fa_candidates
    }

    /// FA候補キャラの処遇を確定する
    pub fn handle_fa_decision(
        &mut self,
        character: Character,
        decision: FaDecision,
        logs: &mut Vec<String>,
    ) -> Option<Character> {
        let tag = BuildTag::analyze(&character);
        match decision {
            FaDecision::Retain => {
                logs.push(format!(
                    " 🤝 [ギルド残留] {} ({}) を引き留めました（稼働スコア再充填）",
                    character.name, tag.name_ja()
                ));
                self.active_members.push(character);
                None
            }
            FaDecision::RetireAsTownNpc => {
                logs.push(format!(
                    " 🏠 [街のNPC化] {} ({}) は冒険の一線を退き、プロンテラ広場の名物鍛冶屋NPCとなりました！ (永続恩恵: 全味方武器ATK +3%)",
                    character.name, tag.name_ja()
                ));
                self.town_bonus.extra_weapon_atk_percent += 0.03;
                self.retired_town_npcs.push(character);
                None
            }
            FaDecision::ApproveTransfer => {
                // 移籍金 = レベル × 1,000 + 装備価値
                let transfer_fee = character.level * 1200 + 15000;
                self.treasury_zeny += transfer_fee;
                logs.push(format!(
                    " ✈️ [FA移籍承認] {} ({}) のライバルリーグへの移籍を承認しました（移籍金: +{} Zeny獲得）",
                    character.name, tag.name_ja(), transfer_fee
                ));
                self.transferred_members.push(character.clone());
                Some(character) // 移籍先ギルドへ渡す
            }
        }
    }
}

/// ライバルギルド
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RivalGuild {
    pub name: String,
    pub roster: Vec<Character>,
}

impl RivalGuild {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            roster: Vec::new(),
        }
    }

    /// シナジー引力（相性スコア）の計算
    /// Why: 自ギルドで活躍した「武器研究BS」と「物理キャリーシーフ」「VITナイト」が、
    /// ライバルギルド内で互いに引き寄せられ、完璧な三位一体パーティーを自動形成する仕組み。
    pub fn calculate_synergy_score(&self, candidate: &Character) -> i32 {
        let candidate_tag = BuildTag::analyze(candidate);
        let mut score = 0;

        for member in &self.roster {
            let m_tag = BuildTag::analyze(member);
            match (candidate_tag, m_tag) {
                // 武器支援 × 物理キャリー = 最高相性
                (BuildTag::WeaponSupport, BuildTag::PhysicalCarry)
                | (BuildTag::PhysicalCarry, BuildTag::WeaponSupport) => score += 60,

                // タンク × キャリー = 鉄板相性
                (BuildTag::Tank, BuildTag::PhysicalCarry)
                | (BuildTag::PhysicalCarry, BuildTag::Tank) => score += 45,

                // タンク × 武器支援 = 安定相性
                (BuildTag::Tank, BuildTag::WeaponSupport)
                | (BuildTag::WeaponSupport, BuildTag::Tank) => score += 35,

                // 同一タグ同士は重複ペナルティ
                (a, b) if a == b => score -= 20,

                _ => score += 10,
            }
        }

        score
    }

    /// FA移籍キャラをシナジー引力によって入団・再鍛錬させる
    pub fn recruit_and_train(&mut self, mut ch: Character, logs: &mut Vec<String>) {
        let tag = BuildTag::analyze(&ch);
        let score = self.calculate_synergy_score(&ch);

        // ライバルギルドでの再鍛錬ボーナス（レベル底上げと最新装備の支給）
        ch.level = ch.level.max(48);
        logs.push(format!(
            " 🏴‍☠️ [シナジー引力] {} ({}) は相性スコア {} の高さを理由に『{}』へ電撃入団！",
            ch.name, tag.name_ja(), score, self.name
        ));
        self.roster.push(ch);
    }
}
