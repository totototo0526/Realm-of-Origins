# Realm of Origins - 開発者向けドキュメント (README_DEV)

## プロジェクト概要
古典的MMORPG（RO風）のステータス・カードビルドの奥深さと、パーティー編成シナジーを検証するためのヘッドレス（CUI）戦闘＆経済シミュレーターです。

## 設計思想
1. **データ駆動（Data-Driven Architecture）**:
   ゲームデータ（職業、スキル、カード、モンスター）はすべて `mods/core/` 以下の RON (Rusty Object Notation) ファイルに外だし。
2. **Modifierベースの柔軟なルール改変**:
   トリガー（`Passive`, `PartyAura`, `OnNormalAttackHit`, `OnBattleEnd`）と効果（`StatBonus`, `RaceDamageBonus`, `AutoSpell`, `PartyWeaponResearch`, `OverchargeBonus`）の合成によりスキルやカード効果を表現。
3. **パーティー編成シナジー ＆ 経済循環（アイテムシンク）**:
   非戦闘型製造・商人職（ブラックスミス等）が参加することによる味方全体の武器攻撃力底上げ（武器研究オーラ）や戦利品換金率向上（オーバーチャージ）に加え、獲得資金と予備装備を鍛冶屋の過剰精錬に投入するアイテムシンクを実装。
4. **簡易ガンビットによる戦術的適応**:
   敵の個体差（迅速型等）や自身のHP状況に応じた条件付き行動（必中攻撃スキル、武器持ち替え等）を自動評価。

## 実装機能サマリー
- **基礎ステータス・計算式 (`src/models.rs`, `src/engine.rs`)**:
  - `Stats`: STR, AGI, VIT, INT, DEX, LUK
  - ATK: STR二乗閾値ボーナス + DEX/5 + 武器ATK
  - ASPD & 行動間隔Tick: AGI/DEXによる攻撃間隔算出
  - HIT & FLEE: レベル + DEX/AGI
  - CRIT: LUK依存の防御力完全無視・必中攻撃
  - DEF: VIT減算固定値防御 + 装備精錬値防御
- **外部データ駆動型ローダー (`src/loader.rs`, `mods/core/*.ron`)**:
  - `jobs.ron`: 職業データ（シーフ、ナイト、ブラックスミス）
  - `cards.ron`: 種族特化カード（ヒドラ: 亜人特効+20%）、オートスペルカード（炎眼: ファイアーボルトLv3発動）等
  - `skills.ron`: 陣形武器研究オーラ、オーバーチャージ、ファイアーボルト、アイアンディフェンス、バッシュ（必中強撃）
  - `monsters.ron`: オークウォーリアー、ハイオーク
- **モンスター個体差テンプレート (`MonsterArchetype`)**:
  - `Standard` (標準), `Swift` (迅速型: 高FLEE/ASPD), `Heavy` (重装型: 高VIT/HP/ATK), `Lucky` (幸運型: 高LUK/高ドロップ)
- **簡易ガンビットシステム (`GambitRule`)**:
  - `Condition`: `TargetArchetypeIs`, `TargetRaceIs`, `SelfHpBelow`, `Always`
  - `Action`: `UseActiveSkill`（必中スキルで高FLEE無効化等）, `SwitchWeapon`, `NormalAttack`
- **ワークオーダー式過剰精錬（クホホ工場）(`src/refinery.rs`)**:
  - 精錬成功率テーブル（+1〜+4は100%、+5以降漸減）
  - 鍛冶屋職能シナジー（DEX/LUK極振りにより最大+15%の成功率底上げ）
  - 限界突破ボーナス:
    - `+7` 以上: ASPD +4%, クリティカル率 +5%
    - `+10` 以上: 防御無視追加ダメージ +120, オートスペル発動率 2倍
  - 失敗時の「クホホ……」ロスト処理と、希少品ロック（`is_locked == true`）安全装置
  - バッチ指示（`WorkOrder`: `BudgetBurn`, `SafeLimit`）
- **3ステップ検証シナリオ (`src/main.rs`)**:
  - Step 1: 初期狩り（資金・ベース武器準備）
  - Step 2: 全自動クホホ工場稼働（20本投入、+10目標バッチ精錬、完成品カード装着＆ロック）
  - Step 3: 過剰精錬武器 ＋ 対迅速型ガンビットによる下克上再挑戦
  - 脳筋2人編成（1489 Tick）vs シナジー初期（1687 Tick）vs シナジー完成形（855 Tick / -49.3%高速化）の3列比較サマリーを出力。
