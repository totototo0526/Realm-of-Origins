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
5. **世代交代とサクセス×ペナント型ループ**:
   完成した先輩キャラによる次世代の高速育成（セルフ養殖）。押し出された古参キャラがシナジー引力でライバルギルドを結成し、模擬GvGで立ちはだかる。

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
  - 限界突破ボーナス（+7 ASPD/クリティカル上昇、+10 防御無視ダメージ+120 / オートスペル発動率2倍）
  - 失敗時の「クホホ……」ロスト処理と、希少品ロック（`is_locked == true`）安全装置
  - バッチ指示（`WorkOrder`: `BudgetBurn`, `SafeLimit`）
- **経験値・レベルアップとビルド成長方針 (`src/progression.rs`)**:
  - モンスター討伐時の経験値獲得とレベルアップテーブル
  - `GrowthPlan`（`AgiCrit`, `VitTank`, `DexLukCrafter`, `IntDexSupport`）による自動ステータス配分
  - レガシー転生ボーナス（世代 `generation` に応じた初期ボーナスポイント加算）
- **ギルド名簿とFA移籍システム (`src/guild.rs`)**:
  - `GuildRoster`: 定員6名、稼働スコア管理、世代交代シフト
  - `BuildTag`: `PhysicalCarry`, `WeaponSupport`, `Tank`, `MagicSupport`, `HighFlee`
  - FA退団選択肢: 街のNPC化（全味方武器ATK+3%の永続恩恵）、移籍承認（移籍金Zeny獲得）
  - シナジー引力（`calculate_synergy_score`）によるライバルギルド『黒曜の旅団』自動結成
- **模擬GvG（3vs3パーティー対抗戦）(`src/engine.rs`)**:
  - プレイヤー現主力 vs かつて放出した古参精鋭連合のPvP戦闘シミュレーション
- **BOM連動型・動的市場システム (`src/market.rs`)**:
  - `MarketItem`: 在庫数に応じたダイナミックプライシング（0.3倍〜3.0倍キャップ、自然回復）
  - BOM（部品表）ツリー連動: 完成品購入時に街の工房が原材料（白ハーブ・鋼鉄・鉄鉱石・石炭等）を連鎖消費するプル型需要連鎖
  - 特需イベントフックによる突発的相場変動
- **控え部隊の裏・自動派遣（オートチェス型ファーミング）(`src/dispatch.rs`)**:
  - 控えメンバー（2軍）による廃鉱山・薬草園への遠征と素材・Zeny自動収集
  - 白ポーションを定常コストとするアイテムシンク
  - 職業・特性タグによる部隊シナジー（商人＋支援＋前衛による「完全自給自足トリオ」: 素材+50%, 消費-50%）
- **対ライバル特化ガンビット＆GvGリベンジ (`src/engine.rs`, `mods/core/skills.ron`)**:
  - 錐効果スキル【ヴァイタルストライク】（敵DEF/VITが高いほど追加割合ダメージを与える装甲貫通スキル）
  - 緊急治癒スキル【サンクチュアリ】（味方のHPが一定割合を下回った際の自動ヒール）
  - 相手のVITをトリガーとするガンビット条件 `Condition::TargetVitAbove(threshold)`
  - 街NPC化した古参鍛冶屋の永続バフ（`TownPassiveBonus`）をプレイヤー側陣形オーラへ合算反映
- **完全データ駆動化（アイテム・レシピ・エリア）(`mods/core/*.ron`, `src/loader.rs`)**:
  - `items_recipes.ron`: 市場アイテム・装備品・BOM製造レシピツリーを外部ファイル化
  - `areas.ron`: 狩場エリア・出現モンスター・推奨Lv・自動派遣ドロップ・消費コストを外部ファイル化
- **統合ワールドステート＆セーブ/ロード (`src/world.rs`)**:
  - 全ゲーム状態を集約する `WorldState`（世代数、ワールド暦、資産、共有倉庫、現役キャラ、同行PT、名簿、市場、派遣、ライバルギルド）
  - RONフォーマットによる `saves/save_data.ron` へのシリアライズ保存とデシリアライズ復元
  - ワールドTick同期進行処理（冒険1回ごとに裏の派遣消化、市場自然回復・BOM補充、ライバル同士の裏リーグ戦と戦力成長が連動）
- **インタラクティブCUIゲームループ ＆ ヘッドレス自動検証モード (`src/main.rs`)**:
  - `cargo run`: プレイヤーがコマンド番号（冒険、編成、精錬所、市場、派遣、レガシー転生・FA、GvG、セーブ/ロード）を選択して遊べるCUI司令部
  - `cargo run -- --headless-demo`: 全システム（初期化〜冒険〜派遣〜市場〜精錬〜転生〜GvG〜セーブ/ロード完全一致検証）を一括実行する回帰テストモード
