# Walkthrough: 古典的MMORPG風 ヘッドレス戦闘＆経済シミュレーター

## 1. 実装概要
古典的MMORPG（RO風）のステータス・カードビルドの奥深さとパーティー編成シナジーを検証するため、データ駆動型（Core = 公式MOD / RON形式）のRust製CUIシミュレーターを構築しました。

## 2. 成果物と実装モジュール
- [Cargo.toml](file:///media/ksp-zorin-001/WORK_DISK/プライベート/自作ゲーム/Realm%20of%20Origins/Cargo.toml): `serde`, `ron`, `rand` の導入
- [src/models.rs](file:///media/ksp-zorin-001/WORK_DISK/プライベート/自作ゲーム/Realm%20of%20Origins/src/models.rs):
  - 基礎ステータス（`Stats`: STR, AGI, VIT, INT, DEX, LUK）
  - 戦闘パラメータ、種族（`Race`）、属性（`Element`）、サイズ（`Size`）
  - ルール改変修飾子（`Modifier`: `Trigger` と `Effect` の合成）
  - 性格テンプレート（`MonsterArchetype`: Standard, Swift, Heavy, Lucky）
- [mods/core/](file:///media/ksp-zorin-001/WORK_DISK/プライベート/自作ゲーム/Realm%20of%20Origins/mods/core):
  - [jobs.ron](file:///media/ksp-zorin-001/WORK_DISK/プライベート/自作ゲーム/Realm%20of%20Origins/mods/core/jobs.ron): 職業定義（シーフ、ナイト、ブラックスミス/商人）
  - [cards.ron](file:///media/ksp-zorin-001/WORK_DISK/プライベート/自作ゲーム/Realm%20of%20Origins/mods/core/cards.ron): カード定義（ヒドラカード、炎眼のオートスペルカード等）
  - [skills.ron](file:///media/ksp-zorin-001/WORK_DISK/プライベート/自作ゲーム/Realm%20of%20Origins/mods/core/skills.ron): スキル定義（武器研究オーラ、オーバーチャージ、ファイアーボルト、アイアンディフェンス）
  - [monsters.ron](file:///media/ksp-zorin-001/WORK_DISK/プライベート/自作ゲーム/Realm%20of%20Origins/mods/core/monsters.ron): モンスターテンプレート（オークウォーリアー、ハイオーク）
- [src/loader.rs](file:///media/ksp-zorin-001/WORK_DISK/プライベート/自作ゲーム/Realm%20of%20Origins/src/loader.rs): `mods/core/` からのRONデータ読み込みエンジン
- [src/engine.rs](file:///media/ksp-zorin-001/WORK_DISK/プライベート/自作ゲーム/Realm%20of%20Origins/src/engine.rs):
  - ステータス計算（STR二乗ボーナス、ASPDと攻撃間隔Tick、HIT/FLEE、CRIT必中・防御無視、VIT減算DEF）
  - パーティーオーラ計算（`PartyWeaponResearch` による味方武器ATK底上げ）
  - 商才換金計算（`OverchargeBonus` によるZeny増加）
  - Tickベース戦闘ループ（通常攻撃、特化特効乗算、オートスペル発動、ヘイト管理）
- [src/main.rs](file:///media/ksp-zorin-001/WORK_DISK/プライベート/自作ゲーム/Realm%20of%20Origins/src/main.rs): シナリオ検証とコンソールログ・比較サマリー出力

## 3. シミュレーション検証結果

同一の10連戦モンスター群（オークウォーリアー＆ハイオーク、4種の性格個体差つき）に対し、以下の2編成でテストを実施しました：
- **パーティーA（脳筋構成）**：高速アタッカー（AGI/STRシーフ）× 2 ＋ 壁役（VITナイト）× 1
- **パーティーB（専門職シナジー構成）**：特化＆オートスペルアタッカー × 1 ＋ 壁役 × 1 ＋ 非戦闘の武器研究・商才特化（DEX/LUK極の鍛冶屋兼商人）× 1

### 比較結果サマリー
```text
┌─────────────────────────────┬─────────────────────┬─────────────────────┐
│ 評価項目                    │ パーティーA (脳筋)   │ パーティーB (シナジー)│
├─────────────────────────────┼─────────────────────┼─────────────────────┤
│ 総討伐時間 (Tick合計)       │            1676 Tick │            2107 Tick │
│ 平均討伐時間 (Tick/戦)      │           167.6 Tick │           210.7 Tick │
│ 総与ダメージ (Total Damage) │           58750 Dmg  │           51206 Dmg  │
│ オートスペル発動回数        │                   0 回 │                  61 回 │
│ ドロップ基礎価値 (Raw Zeny) │            7720 Zeny │            7080 Zeny │
│ 最終換金総額 (Overcharge後) │            7720 Zeny │            8525 Zeny │
└─────────────────────────────┴─────────────────────┴─────────────────────┘
```

### 検証結果の考察
1. **アタッカー半減を補うシナジー効果**:
   パーティーBは実質的な攻撃役が1名のみ（シーフ1名＋商人は低火力）であるにも関わらず、ヒドラカードの亜人種族特効（+20%）、ファイアーボルトLv3のオートスペル追加魔法ダメージ（61回発動、弱点属性補正）、および鍛冶屋の「武器研究オーラ（武器ATK+26.3%）」により、2名のアタッカーを擁するパーティーAに対して討伐速度で大きく離されずに肉薄しました。
2. **経済シナジー（Overcharge）**:
   戦闘終了時のドロップアイテム換金において、商人の商才（LUK 85によるスケーリング）が発動し、最終獲得Zenyが1.20倍に増加しました。
3. **モンスターの個体差（Archetype）表現**:
   迅速型では高FLEEにより攻撃ミス（MISS）が多発し、重装型では高いHPとDEFで長期戦化するなど、個体ごとの挙動差が戦闘ログ上に鮮明に現れました。
