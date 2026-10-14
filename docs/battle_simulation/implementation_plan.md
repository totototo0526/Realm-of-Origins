# Implementation Plan: 古典的MMORPG風 ヘッドレス戦闘＆経済シミュレーター

## 1. 概要
古典的MMORPG（RO風）のステータス・カードビルドの奥深さと、パーティー編成シナジーを検証するためのヘッドレス（CUI）戦闘＆経済シミュレーターをRustで構築します。
将来的なMOD拡張を見据え、ゲーム内の職業・スキル・カード・モンスターの定義はすべて `mods/core/` 配下の外部ファイル（RON形式）から読み込むデータ駆動型（Core = 公式MOD）アーキテクチャを採用します。

## 2. アーキテクチャとモジュール設計

### モジュール構成
- `src/main.rs`: エントリーポイント。初期データのロード、モンスター10連戦の生成、パーティーA vs パーティーBのシミュレーション実行、結果の比較ログ表示。
- `src/models.rs`: ゲーム内の全データモデル定義。
  - `Stats`: `str_stat`, `agi`, `vit`, `int`, `dex`, `luk` (u16)
  - `Race`, `Element`, `Size`, `StatType`
  - `Trigger`: `Passive`, `PartyAura`, `OnNormalAttackHit { chance: f32 }`, `OnBattleEnd`
  - `Effect`:
    - `StatBonus { stat: StatType, flat: i32, percent: f32 }`
    - `RaceDamageBonus { race: Race, percent: f32 }`
    - `AutoSpell { skill_id: String, level: u8 }`
    - `PartyWeaponResearch { base_bonus: f32, scaling_stats: Vec<StatType> }`
    - `OverchargeBonus { base_bonus: f32, scaling_stat: StatType }`
  - `Modifier`: `Trigger` と `Effect` のペア
  - `Card`, `Equipment`, `Skill`, `Job`, `Character`
  - `MonsterTemplate`, `MonsterArchetype` (`Standard`, `Swift`, `Heavy`, `Lucky`), `SpawnedMonster`
- `src/loader.rs`: `mods/core/` 内のRONファイル（`jobs.ron`, `cards.ron`, `skills.ron`, `monsters.ron`）をパース・読み込むモジュール。
- `src/engine.rs`:
  - ステータス計算:
    - 基礎ATK: $STR + (STR / 10)^2 + DEX / 5 + (\text{武器ATK} \times \text{武器研究倍率}) + \text{精錬}$
    - ASPD: 基本ASPD + AGI/DEX補正。攻撃間隔Tick（ASPDが高いほどTickが短い）
    - HIT: $Lv + DEX$
    - FLEE: $Lv + AGI$
    - CRIT: $1 + LUK \times 0.3\%$（防御無視＋必中）
    - DEF: 割合軽減（装備DEF）＋ 固定値軽減（VIT）
  - パーティーオーラ計算:
    - `PartyWeaponResearch`: 補正倍率 = $1.0 + \text{base\_bonus} \times (\sum \text{scaling\_stats} / 100.0)$
    - `OverchargeBonus`: ドロップ換金倍率 = $1.0 + \text{base\_bonus} \times (\text{scaling\_stat} / 100.0)$
  - 戦闘ループ（Tickベース）:
    - 各キャラおよびモンスターの行動ディレイ（Tick）を管理。
    - 攻撃時、HIT vs FLEE（命中率 $80 + HIT - FLEE$、下限5%、上限95%）とCRIT判定。
    - 種族特化カード補正（同種加算、異種乗算）。
    - 命中時の `AutoSpell` 判定（確率で追加魔法ダメージ発動）。
    - ヘイト管理（壁役ナイトがターゲットを維持）。
  - 戦闘終了処理:
    - ドロップ品抽選、Overchargeによる換金ボーナス適用。
- `mods/core/`:
  - `jobs.ron`, `cards.ron`, `skills.ron`, `monsters.ron`

## 3. 検証シナリオ
1. **同一のモンスター10体セット**を生成（`OrcWarrior` をベースに `Standard`, `Swift`, `Heavy`, `Lucky` をランダム付与）。
2. **パーティーA（脳筋構成）**：
   - Thief A (STR/AGI型、通常武器)
   - Thief B (STR/AGI型、通常武器)
   - Knight (VIT/STR型、高耐久)
3. **パーティーB（専門職シナジー構成）**：
   - Thief A (STR/AGI型、ヒドラ特化カード＋オートスペルカード挿し短剣)
   - Knight (VIT/STR型、高耐久)
   - Blacksmith/Merchant (DEX/LUK極振り、武器研究Lv10、OverchargeLv10)
4. 両パーティーで10連戦を行い、戦闘詳細ログおよび以下の指標のサマリー比較を表示：
   - 総討伐Tick数（短いほど高速）
   - 平均討伐Tick数
   - 総与ダメージ
   - オートスペル発動回数
   - 獲得した戦利品の換金総額（Overcharge補正込み）

## 4. 開発ステップ
1. ドキュメント整備 (`agent.md`, `README_DEV.md`)
2. `Cargo.toml` の作成とクレート設定
3. `src/models.rs` の実装
4. `mods/core/` のデータファイル（RON）作成
5. `src/loader.rs` の実装
6. `src/engine.rs` の実装
7. `src/main.rs` の実装
8. `cargo check` & `cargo run` で動作検証
9. `docs/battle_simulation/walkthrough.md` の作成とドキュメント更新
