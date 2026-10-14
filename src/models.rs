use serde::{Deserialize, Serialize};

/// 基礎ステータス6種（ROクラシック仕様）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StatType {
    Str,
    Agi,
    Vit,
    Int,
    Dex,
    Luk,
}

/// 基礎ステータス値
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Stats {
    #[serde(rename = "str", alias = "str_stat")]
    pub str_stat: u16,
    pub agi: u16,
    pub vit: u16,
    pub int: u16,
    pub dex: u16,
    pub luk: u16,
}

impl Stats {
    pub fn new(str_stat: u16, agi: u16, vit: u16, int: u16, dex: u16, luk: u16) -> Self {
        Self {
            str_stat,
            agi,
            vit,
            int,
            dex,
            luk,
        }
    }

    pub fn get(&self, stat: StatType) -> u16 {
        match stat {
            StatType::Str => self.str_stat,
            StatType::Agi => self.agi,
            StatType::Vit => self.vit,
            StatType::Int => self.int,
            StatType::Dex => self.dex,
            StatType::Luk => self.luk,
        }
    }

    pub fn add(&mut self, stat: StatType, value: i32) {
        let current = self.get(stat) as i32;
        let new_val = (current + value).max(0) as u16;
        match stat {
            StatType::Str => self.str_stat = new_val,
            StatType::Agi => self.agi = new_val,
            StatType::Vit => self.vit = new_val,
            StatType::Int => self.int = new_val,
            StatType::Dex => self.dex = new_val,
            StatType::Luk => self.luk = new_val,
        }
    }
}

/// 種族
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Race {
    Demihuman, // 人間・亜人
    Undead,    // 不死
    Brute,     // 動物
    Plant,     // 植物
    Insect,    // 昆虫
    Fish,      // 魚類
    Demon,     // 悪魔
    Formless,  // 無形
    Angel,     // 天使
    Dragon,    // 竜
}

/// 属性
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Element {
    Neutral,
    Water,
    Earth,
    Fire,
    Wind,
    Poison,
    Holy,
    Shadow,
    Ghost,
    Undead,
}

/// サイズ
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Size {
    Small,
    Medium,
    Large,
}

/// 装備スロット
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EquipSlot {
    Weapon,
    Shield,
    Armor,
    Garment,
    Footwear,
    Accessory,
}

/// 効果の発動トリガー
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Trigger {
    /// 常時発動（自己ステータス、特攻ボーナス等）
    Passive,
    /// パーティー全体に恩恵を与えるオーラ
    PartyAura,
    /// 通常攻撃命中時に確率発動
    OnNormalAttackHit { chance: f32 },
    /// 戦闘終了時に発動（ドロップ精算など）
    OnBattleEnd,
}

/// スキルやカードの効果実体
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Effect {
    /// ステータス固定値・割合加算
    StatBonus {
        stat: StatType,
        flat: i32,
        percent: f32,
    },
    /// 特定種族への与ダメージボーナス（同種は加算、異種は乗算される）
    RaceDamageBonus {
        race: Race,
        percent: f32,
    },
    /// 通常攻撃命中時のオートスペル発動（スキルIDと発動レベル）
    AutoSpell {
        skill_id: String,
        level: u8,
    },
    /// パーティー全体の武器攻撃力を底上げする研究効果
    /// 補正倍率 = 1.0 + base_bonus * (参照ステータス合計 / 100.0)
    PartyWeaponResearch {
        base_bonus: f32,
        scaling_stats: Vec<StatType>,
    },
    /// 戦闘終了時の換金額を底上げする商才効果
    /// 補正倍率 = 1.0 + base_bonus * (参照ステータス / 100.0)
    OverchargeBonus {
        base_bonus: f32,
        scaling_stat: StatType,
    },
}

/// データ駆動のルール改変用修飾子（Modifier）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Modifier {
    pub id: String,
    pub name: String,
    pub trigger: Trigger,
    pub effect: Effect,
}

/// カード定義
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Card {
    pub id: String,
    pub name: String,
    pub slot: EquipSlot,
    pub modifiers: Vec<Modifier>,
}

fn default_slots() -> u8 {
    4
}

/// 装備品
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Equipment {
    pub id: String,
    pub name: String,
    pub slot: EquipSlot,
    pub weapon_atk: u32,
    #[serde(default)]
    pub refine: u8, // refine_level (0〜15)
    #[serde(default = "default_slots")]
    pub slots: u8, // 最大スロット数
    #[serde(default)]
    pub cards: Vec<Card>,
    #[serde(default)]
    pub modifiers: Vec<Modifier>,
    #[serde(default)]
    pub is_locked: bool, // 希少品保護ロック（破壊リスクのある精錬を拒否）
}

impl Equipment {
    #[allow(dead_code)]
    pub fn new(id: &str, name: &str, slot: EquipSlot, weapon_atk: u32, refine: u8) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            slot,
            weapon_atk,
            refine,
            slots: 4,
            cards: vec![],
            modifiers: vec![],
            is_locked: false,
        }
    }
}

/// スキル定義
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub level: u8,
    pub sp_cost: u32,
    /// オートスペル用等の魔法倍率（例: 1.0 = 100% MATK）
    #[serde(default)]
    pub magic_multiplier: Option<f32>,
    /// 物理攻撃スキルの倍率（例: 2.5 = 250% ATK）
    #[serde(default)]
    pub physical_multiplier: Option<f32>,
    /// 必中判定フラグ（HIT vs FLEE判定をバイパスし100%命中）
    #[serde(default)]
    pub is_sure_hit: bool,
    #[serde(default)]
    pub modifiers: Vec<Modifier>,
}

/// 簡易ガンビット：発動条件
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Condition {
    /// ターゲットモンスターの個体差性格（例: 迅速型 Swift）
    TargetArchetypeIs(MonsterArchetype),
    /// ターゲットモンスターの種族
    TargetRaceIs(Race),
    /// 自身のHP割合が閾値未満（例: 0.3 = 30%以下）
    SelfHpBelow(f32),
    /// 無条件（常に真）
    Always,
}

/// 簡易ガンビット：実行アクション
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Action {
    /// 指定のアクティブスキルを使用
    UseActiveSkill(String),
    /// 指定の武器に持ち替え
    SwitchWeapon(String),
    /// 通常攻撃
    NormalAttack,
}

/// 簡易ガンビットルール定義
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GambitRule {
    pub id: String,
    pub priority: u32, // 数値が小さいほど高優先度
    pub condition: Condition,
    pub action: Action,
}

/// 職業定義
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub name: String,
    pub base_hp: u32,
    pub hp_factor: f32,
    pub base_aspd: f32,
    pub modifiers: Vec<Modifier>,
}

fn default_generation() -> u8 {
    1
}

/// プレイヤーキャラクター
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Character {
    pub id: String,
    pub name: String,
    pub job_id: String,
    pub level: u32,
    pub base_stats: Stats,
    pub equipments: Vec<Equipment>,
    pub learned_skills: Vec<Skill>,
    #[serde(default)]
    pub gambit_rules: Vec<GambitRule>,
    #[serde(default = "default_generation")]
    pub generation: u8,
    #[serde(default)]
    pub exp: u64,
    #[serde(default)]
    pub activity_score: u32,
}

impl Character {
    #[allow(dead_code)]
    pub fn new_with_gen(
        id: &str,
        name: &str,
        job_id: &str,
        level: u32,
        generation: u8,
        base_stats: Stats,
    ) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            job_id: job_id.to_string(),
            level,
            base_stats,
            equipments: vec![],
            learned_skills: vec![],
            gambit_rules: vec![],
            generation,
            exp: 0,
            activity_score: 100,
        }
    }
}

/// モンスターのドロップアイテム定義
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DropItem {
    pub id: String,
    pub name: String,
    pub base_value: u32,
    pub drop_rate: f32, // 0.0 ~ 1.0
}

/// モンスターの性格テンプレート（個体差）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MonsterArchetype {
    Standard, // 標準型: 基本性能
    Swift,    // 迅速型: AGI/DEX特化、回避と攻撃速度が高い
    Heavy,    // 重装型: VIT/STR特化、HP・防御力・攻撃力が高い
    Lucky,    // 幸運型: LUK特化、クリティカル耐性・高ドロップ
}

impl MonsterArchetype {
    pub fn name_ja(&self) -> &'static str {
        match self {
            MonsterArchetype::Standard => "標準型",
            MonsterArchetype::Swift => "迅速型",
            MonsterArchetype::Heavy => "重装型",
            MonsterArchetype::Lucky => "幸運型",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            MonsterArchetype::Standard => "均整の取れたステータスを持つ標準個体",
            MonsterArchetype::Swift => "回避と攻撃頻度が極めて高く、攻撃を当てにくい俊足個体",
            MonsterArchetype::Heavy => "高いVITと強靭な装甲を誇り、攻撃が重い強靭個体",
            MonsterArchetype::Lucky => "高い幸運を持ち、ドロップ品や完全回避に優れる希少個体",
        }
    }
}

/// モンスターテンプレート（RONから読み込む基礎データ）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MonsterTemplate {
    pub id: String,
    pub name: String,
    pub level: u32,
    pub race: Race,
    pub element: Element,
    pub size: Size,
    pub base_stats: Stats,
    pub base_hp: u32,
    pub base_exp: u32,
    pub drops: Vec<DropItem>,
}

/// 個体差が割り振られてスポーンしたモンスター実体
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpawnedMonster {
    pub template_id: String,
    pub name: String,
    pub archetype: MonsterArchetype,
    pub level: u32,
    pub race: Race,
    pub element: Element,
    pub size: Size,
    pub stats: Stats,
    pub current_hp: i32,
    pub max_hp: u32,
    pub drops: Vec<DropItem>,
}
