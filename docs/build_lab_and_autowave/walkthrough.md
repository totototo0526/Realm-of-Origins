# フェーズ9 実装完了報告 (Walkthrough)

## 1. 概要
フェーズ9では、2.5DクォータービューGUIを単なるバトル鑑賞画面から、プレイヤーが**「変なビルドやカードの組み合わせを思いつき、実際に画面上で組み立てて自動周回させ、DPSや稼ぎ効率の変化を実験・発見できる本物のハクスラ＆ギルド運営ゲーム」**へと深化させました。

以下の4つのコア機能がGUI上で完全統合され、画面クリックのみで全ゲームサイクルを実験・検証できるようになりました。

---

## 2. 実装された機能と操作フロー

### ① Auto-Wave連戦 ＆ リアルタイムDPS/時給メトリクスバー
- **HUDメトリクスバー（戦場上部）**:
  - `[Wave X]`: 現在の連戦Wave数
  - `[DPS: X.X]`: 総ダメージ / 経過Tick のリアルタイム集計
  - `[EXP: X/100T]`: 100Tickあたりの経験値効率（時給メーター）
  - `[命中: X.X%]`: チームA（プレイヤー）の攻撃試行に対する命中率
  - `[会心: X.X%]`: チームAのクリティカル発動率
  - `[Auto-Wave: ON/OFF]`: ワンクリックで自動周回モードを切り替え
- **自動周回ループとワールドターン連動**:
  - `[Auto-Wave: ON]` の状態で敵を全滅させると、画面上に `(次群まで 2.0s)` のカウントダウンが表示。
  - 2秒経過後、`world.advance_turn(elapsed_ticks, &db)` が自動実行され、裏の「控え部隊の自動派遣」「ダイナミックプライシング市場の価格変動」「ライバルギルドの裏リーグ戦」がリアルタイムに進行。
  - 次のモンスター群が自動スポーンし、連戦が継続。

### ② タブ1 `[戦場/編成]`：パーティー全員のカスタマイズと手動ステ振り（ビルド実験）
- **編集対象メンバー切り替え**:
  - `[1. 主人公: クロード]` `[2. 同行: バルガス]` `[3. 同行: オットー]` の3ボタンで即座に切り替え可能。
  - 同行者の武器・ガンビット・陣形ロールも画面上で自由に変更可能。
- **手動ステータス割り振り（ビルド実験）**:
  - `STR`, `AGI`, `VIT`, `INT`, `DEX`, `LUK` の各項目に `[+1]`, `[+10]` ボタンを配備。
  - 極振りボタン `[STR極]`, `[AGI極]`, `[VIT極]`, `[INT極]`, `[DEX極]`, `[LUK極]` で一気にステータスを+50強化可能。
  - `[再振り]` ボタンで全ステータスを初期値(10)にリセットし、自由な振り直しが可能。
  - **戦闘セッションへの即時同期**: ステータスを変更した瞬間、`SimulationEngine::calculate_combat_stats` によりHP・ATK・DEF・ASPD・HIT・FLEE・CRITが再計算され、現在進行中の戦闘ユニットにリアルタイム反映されます。

### ③ タブ2 `[工房/精錬]`：カード脱着・合成と「クホホ」のライブ連携
- **所持カード一覧と自由な装着**:
  - 倉庫内のカード一覧を表示（ヒドラC、炎眼オートスペルC、ソルジャースケルトンC、ドレインリアーC、アンドレC）。
  - 各カードの `[+装着]` ボタンをクリックすると、選択中武器の空きスロットへ即座にカードを装着。
  - 異種乗算（ヒドラC×ドレインリアーC）やオートスペル特化（炎眼C×ソルスケC）などの自由なビルドを構築可能。
- **カード初期化**:
  - `[全カード取り外し (初期化)]` ボタンで装着カードをすべて解除。
  - カードの脱着時も即座に戦闘セッションのユニットステータスに反映。

### ④ タブ4 `[名簿/転生]`：狩場の生態系シフト分析（Solver）とライバル偵察＆指名GvG
- **狩場のメタ適応（生態系シフトSolver）**:
  - 直前の戦闘セッションにおける近接物理ダメージと遠距離/魔法ダメージの比率を解析。
  - プレイヤーが物理攻撃に偏重している場合、敵モンスター群は `Heavy`（重装型・高VIT・高DEF）の比率を75%に増加。
  - 遠距離や魔法に偏重している場合、敵モンスター群は `Swift`（迅速型・高AGI・高回避）の比率を75%に増加。
  - 画面上部およびチャットログに `[!] 生態系シフト: 敵が物理対策（重装型・高DEF増加）を開始しました！` と警告バナーが表示。
- **ライバルギルド偵察と指名GvG開戦**:
  - `[黒曜の旅団]` `[黄金の盾]` `[灰燼の学派]` の切り替えタブで、各ギルドのロスター（メンバー名、世代、職業、主武器、BuildTag特性）を偵察可能。
  - かつて自軍からFA移籍したメンバーには `[★元自軍]` バッジを表示。
  - `[★ このライバルを指名して空間GvG開戦 (闘技場)]` ボタンをクリックすると、そのライバルギルドの編成との空間GvG闘技場バトルが即座に開戦し、戦場タブへ自動遷移。

---

## 3. テストと動作検証

### 自動テスト（`cargo test`）
```bash
running 12 tests
test dispatch::tests::test_dispatch_synergy_and_farming ... ok
test hex_grid::tests::test_astar_pathfinding_around_building ... ok
test hex_grid::tests::test_astar_pathfinding_with_attack_range ... ok
test engine::tests::test_spatial_knockback_step ... ok
test hex_grid::tests::test_building_footprint_blocks_hexes ... ok
test hex_grid::tests::test_hex_screen_pixel_conversion ... ok
test hex_grid::tests::test_hex_distance_and_neighbors ... ok
test market::tests::test_market_dynamic_pricing_and_bom_pull ... ok
test world::tests::test_world_advance_turn_sync ... ok
test engine::tests::test_spatial_gvg_simulation ... ok
test engine::tests::test_spatial_chokepoint_defense_and_aoe ... ok
test world::tests::test_world_state_save_and_load ... ok

test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
```

### ヘッドレスデモ検証（`cargo run -- --headless-demo`）
- 10ステップの統合シナリオ（隘路防衛、AoE、ノックバック、BOM動的市場、過剰精錬、レガシー世代交代、空間GvG、セーブ/ロード往復一致）がすべてSUCCESSで完走することを確認。
