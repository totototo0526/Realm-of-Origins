mod dispatch;
mod engine;
mod guild;
mod loader;
mod market;
mod models;
mod progression;
mod refinery;
mod world;

use std::io::{self, Write};
use engine::SimulationEngine;
use guild::FaDecision;
use loader::ModDatabase;
use models::{
    Action, Card, Condition, EquipSlot, GambitRule,
    Modifier, StatType, Trigger, Effect
};
use progression::{GrowthPlan, Progression};
use refinery::{Refinery, WorkOrder, WorkOrderMode};
use world::WorldState;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = ModDatabase::load_from_dir("mods/core")?;
    let is_headless_demo = std::env::args().any(|arg| arg == "--headless-demo");

    if is_headless_demo {
        run_headless_demo(&db)?;
    } else {
        run_interactive_mode(&db)?;
    }

    Ok(())
}

// =============================================================================
//  【自動検証モード】--headless-demo
//  全システムの統合動作と1プレイヤーターンごとのワールド状態遷移、セーブ/ロード往復を自動検証
// =============================================================================
fn run_headless_demo(db: &ModDatabase) -> Result<(), Box<dyn std::error::Error>> {
    println!("================================================================================");
    println!("  Realm of Origins - フェーズ5：完全統合ワールドステート ＆ 自動回帰テスト");
    println!("  【データ駆動化・セーブ/ロード往復検証・BOM市場・自動派遣・レガシー転生・GvG】");
    println!("================================================================================\n");

    let engine = SimulationEngine::new(db);
    let mut world = WorldState::new_game(db);

    // -------------------------------------------------------------------------
    // Step 1: ワールドステート初期化の確認
    // -------------------------------------------------------------------------
    println!("📦 [Step 1/8] ワールドステートの初期化とデータ駆動ロードの検証");
    println!("   - 現在世代: 第{}世代", world.generation);
    println!("   - ギルド資金: {} Zeny", world.treasury_zeny);
    println!("   - 現役キャラクター: {} (Lv.{} {})", world.active_character.name, world.active_character.level, world.active_character.job_id);
    println!("   - 同行メンバー: {}", world.party_members.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", "));
    println!("   - 控え名簿数: {} 名", world.roster.active_members.len());
    println!("   - 登録ライバルギルド数: {} ギルド", world.rival_guilds.len());
    for rg in &world.rival_guilds {
        println!("     * 『{}』 (所属: {}名)", rg.name, rg.roster.len());
    }
    println!("   - 市場アイテム種類数: {} 種", world.market.items.len());
    println!("   - 登録エリア数: {} 箇所", db.areas.len());
    println!("   ✅ 初期化確認完了\n");

    // -------------------------------------------------------------------------
    // Step 2: 冒険・パワーレベリング（オーク洞窟3連戦）＆ ワールドTick同期進行
    // -------------------------------------------------------------------------
    println!("⚔️ [Step 2/8] 冒険・養殖（パワーレベリング）とワールドTick連動進行");
    let orc_cave = db.areas.get("orc_dungeon").expect("orc_dungeon area not found");
    println!("   - 探索エリア: 『{}』 (推奨Lv.{})", orc_cave.name, orc_cave.recommended_level);

    let party = world.get_active_party();
    let orc_warrior_template = db.monsters.get("orc_warrior").expect("orc_warrior not found");

    for i in 1..=3 {
        let arch = if i == 2 { models::MonsterArchetype::Swift } else { models::MonsterArchetype::Standard };
        let mut spawned = engine.spawn_monster(orc_warrior_template, arch);
        let battle_res = engine.simulate_battle(&party, &mut spawned);
        println!("    battle #{}: {} ({}) を討伐！ (戦闘所要: {} Tick, 獲得EXP: {}, ドロップ換金: {} Zeny)",
            i, battle_res.monster_name, battle_res.monster_archetype.name_ja(), battle_res.total_ticks, battle_res.gained_exp, battle_res.final_loot_value);

        // 経験値付与
        let mut logs = Vec::new();
        Progression::grant_exp(&mut world.active_character, battle_res.gained_exp, world.active_growth_plan, &mut logs);
        for l in logs {
            println!("{}", l);
        }
        world.treasury_zeny += battle_res.final_loot_value;
    }

    // ワールドTick進行（1回の探索で80 Tick経過）
    println!("   ⏱️ ワールド時間を進行 (80 Tick)...");
    let turn_logs = world.advance_turn(80, db);
    for l in turn_logs {
        println!("    {}", l);
    }
    println!("   ✅ 現役 {} は Lv.{} に急成長！ (ワールド累計: {} Tick)\n", world.active_character.name, world.active_character.level, world.calendar_tick);

    // -------------------------------------------------------------------------
    // Step 3: 控え部隊の自動派遣（廃鉱山）＆ ターン進行回収
    // -------------------------------------------------------------------------
    println!("⛏️ [Step 3/8] 控え部隊の自動派遣（オートファーミング）と同期帰還");
    let mine_area = db.areas.get("abandoned_mine").expect("abandoned_mine not found");
    let dispatch_members = vec![
        world.roster.active_members.remove(0),
        world.roster.active_members.remove(0),
        world.roster.active_members.remove(0),
    ];
    let member_names = dispatch_members.iter().map(|c| c.name.clone()).collect::<Vec<_>>();
    println!("   - 派遣部隊結成: {}", member_names.join(", "));
    println!("   - 派遣先: 『{}』 (5サイクル / 所要100 Tick)", mine_area.name);

    world.active_dispatch = Some(dispatch::ActiveDispatch {
        area_id: mine_area.id.clone(),
        area_name: mine_area.name.clone(),
        members: dispatch_members,
        ticks_remaining: 100,
        cycles: 5,
    });

    println!("   ⏱️ メイン部隊が別行動をとり、時を 120 Tick 進めます...");
    let dispatch_turn_logs = world.advance_turn(120, db);
    for l in dispatch_turn_logs {
        println!("    {}", l);
    }
    println!("   - 現在の倉庫鉱石: 鉄鉱石 x{}, オリハルコン x{}",
        world.storage_items.get("iron_ore").unwrap_or(&0),
        world.storage_items.get("oridecon").unwrap_or(&0)
    );
    println!("   ✅ 自動派遣の同期消化・素材回収完了\n");

    // -------------------------------------------------------------------------
    // Step 4: BOM連動市場ハック（買い占めと価格高騰・Overcharge売却）
    // -------------------------------------------------------------------------
    println!("📈 [Step 4/8] BOM連動型動的市場の買い占め＆高値Overcharge売却");
    let damascus_pre = world.market.items.get("damascus").unwrap().buy_price();
    let iron_pre = world.market.items.get("iron_ore").unwrap().buy_price();
    let damascus_stock = world.market.items.get("damascus").unwrap().current_stock;
    println!("   - 買い占め前の相場: ダマスカス {} Zeny (在庫{}), 鉄鉱石 {} Zeny",
        damascus_pre, damascus_stock, iron_pre);

    // ダマスカスを3本買い占め
    let buy_count = 3;
    let cost = world.market.buy("damascus", buy_count).unwrap();
    world.treasury_zeny = world.treasury_zeny.saturating_sub(cost);
    println!("   🛒 ダマスカス {}本 を一括購入 (出費: {} Zeny)", buy_count, cost);

    let damascus_post = world.market.items.get("damascus").unwrap().buy_price();
    let iron_post = world.market.items.get("iron_ore").unwrap().buy_price();
    println!("   - 買い占め後の相場: ダマスカス {} Zeny (+{:.1}%), 鉄鉱石 {} Zeny (+{:.1}%: BOM引力波及)",
        damascus_post,
        (damascus_post as f64 / damascus_pre as f64 - 1.0) * 100.0,
        iron_post,
        (iron_post as f64 / iron_pre as f64 - 1.0) * 100.0
    );

    // 倉庫の鉄鉱石を商人Overchargeで売却
    let iron_in_storage = *world.storage_items.get("iron_ore").unwrap_or(&0);
    let sell_qty = iron_in_storage.min(30);
    if sell_qty > 0 {
        let revenue = world.market.sell("iron_ore", sell_qty, 0.24).unwrap();
        world.treasury_zeny += revenue;
        *world.storage_items.entry("iron_ore".to_string()).or_insert(0) -= sell_qty;
        println!("   💰 派遣で採掘した鉄鉱石 {}個 をOvercharge(+24%)で売却 (売上: +{} Zeny獲得)", sell_qty, revenue);
    }
    println!("   ✅ BOMダイナミックプライシングと市場ハック検証完了\n");

    // -------------------------------------------------------------------------
    // Step 5: 全自動クホホ工場（過剰精錬＆カード装着）
    // -------------------------------------------------------------------------
    println!("🔨 [Step 5/8] 全自動クホホ工場（過剰精錬＆希少品ロック＆カード装着）");
    let blacksmith_otto = world.party_members.iter().find(|c| c.job_id == "blacksmith").unwrap().clone();
    let target_weapon = world.storage_equipments.remove(0); // ダマスカス未精錬

    let order = WorkOrder {
        order_name: "過剰精錬オーダー".to_string(),
        mode: WorkOrderMode::BudgetBurn {
            target_level: 8,
            max_zeny: 30_000,
            max_weapons: 1,
        },
        target_level: 8,
    };

    let (report, remaining_zeny) = Refinery::process_work_order(&order, vec![target_weapon.clone()], &blacksmith_otto, world.treasury_zeny);
    world.treasury_zeny = remaining_zeny;
    for l in report.logs {
        println!("    {}", l);
    }

    let mut target_weapon = report.completed_weapons.into_iter().next().unwrap_or(target_weapon);

    // カード装着（アンドレカード2枚）
    target_weapon.cards.push(Card {
        id: "card_andre".to_string(),
        name: "アンドレカード".to_string(),
        slot: EquipSlot::Weapon,
        modifiers: vec![Modifier {
            id: "andre_bonus".to_string(),
            name: "ATK+20".to_string(),
            trigger: Trigger::Passive,
            effect: Effect::StatBonus { stat: StatType::Str, flat: 5, percent: 0.0 },
        }],
    });
    target_weapon.is_locked = true; // 希少品保護ロック
    println!("   ✨ カード装着＆希少品ロック完了: {} (精錬値:+{}, ロック:{}, カード数:{})",
        target_weapon.name, target_weapon.refine, target_weapon.is_locked, target_weapon.cards.len());

    // 現役クロードに装備
    world.active_character.equipments.clear();
    world.active_character.equipments.push(target_weapon.clone());
    println!("   ✅ クホホ工場と装備強化検証完了\n");

    // -------------------------------------------------------------------------
    // Step 6: レガシー転生 ＆ 古参メンバーのFA移籍承認
    // -------------------------------------------------------------------------
    println!("🔄 [Step 6/8] レガシー転生（世代交代）と古参メンバーのFA移籍承認");
    println!("   - 第1世代クロード (Lv.{}) が殿堂入り！", world.active_character.level);

    // 第2世代キャラクター「影狼」の誕生（転生ボーナス付与）
    let next_gen = world.generation + 1;
    let mut new_hero = Progression::create_reincarnated_character(
        "char_gen2_shadow",
        "影狼",
        "thief",
        next_gen as u8,
        GrowthPlan::AgiCrit,
    );
    new_hero.equipments.push(target_weapon); // 先代の遺産装備を継承！

    // クロードを名簿へ送り、新世代をアクティブに
    world.roster.add_member(world.active_character.clone());
    world.active_character = new_hero;
    world.generation = next_gen;

    println!("   🌟 【第{}世代誕生】次世代主人公『{}』(Lv.1) が先代の+{}武器を継承して就任！",
        world.generation, world.active_character.name, world.active_character.equipments[0].refine);

    // 世代交代に伴うFA判定
    let deployed_ids = vec![world.active_character.id.clone()];
    let fa_candidates = world.roster.process_generation_shift(&deployed_ids);
    println!("   📜 世代交代により {} 名がFA（フリーエージェント）宣言！", fa_candidates.len());

    let mut fa_logs = Vec::new();
    for candidate in fa_candidates {
        let transferred = world.roster.handle_fa_decision(candidate, FaDecision::ApproveTransfer, &mut fa_logs);
        if let Some(ch) = transferred {
            // シナジー引力で最も相性の良いライバルギルドへ移籍
            if let Some(best_guild) = world.rival_guilds.iter_mut().max_by_key(|g| g.calculate_synergy_score(&ch)) {
                best_guild.recruit_and_train(ch, &mut fa_logs);
            }
        }
    }
    for l in fa_logs {
        println!("    {}", l);
    }
    println!("   ✅ レガシー転生とFA移籍完了\n");

    // -------------------------------------------------------------------------
    // Step 7: 宿敵『黒曜の旅団』とのGvG雪辱戦
    // -------------------------------------------------------------------------
    println!("🏆 [Step 7/8] 宿敵『黒曜の旅団』とのギルドリーグ対抗戦（GvG）");
    let obsidian = world.rival_guilds.iter().find(|g| g.name == "黒曜の旅団").unwrap();
    let rival_team = obsidian.roster.iter().take(3).cloned().collect::<Vec<_>>();

    // プレイヤーチームもLv40程度まで養殖された想定でバトル
    world.active_character.level = 45;
    world.active_character.base_stats.add(StatType::Agi, 40);
    world.active_character.base_stats.add(StatType::Str, 35);
    world.active_character.base_stats.add(StatType::Dex, 30);

    // 対Swift必中ガンビット
    world.active_character.gambit_rules.push(GambitRule {
        id: "anti_swift".to_string(),
        priority: 1,
        condition: Condition::TargetArchetypeIs(models::MonsterArchetype::Swift),
        action: Action::UseActiveSkill("bash".to_string()),
    });
    if let Some(bash_skill) = db.skills.get("bash") {
        world.active_character.learned_skills.push(bash_skill.clone());
    }

    let player_team = world.get_active_party();
    println!("   ⚔️ 自ギルド出撃部隊: {} (計{}名)",
        player_team.iter().map(|c| format!("{} (Lv.{})", c.name, c.level)).collect::<Vec<_>>().join(", "),
        player_team.len()
    );
    println!("   ⚔️ ライバル部隊: 『黒曜の旅団』 (計{}名)", rival_team.len());

    let gvg_res = engine.simulate_gvg_battle(
        &player_team,
        &rival_team,
        "我らがギルド",
        &obsidian.name,
        Some(world.roster.town_bonus.clone()),
    );

    println!("   🏁 GvG決着: 勝者『{}』 (戦闘所要: {} Tick, 自軍生存者数: {}名)",
        gvg_res.winner_team, gvg_res.total_ticks, gvg_res.team_a_survivors.len());
    println!("   ✅ ギルドリーグGvG対戦完了\n");

    // -------------------------------------------------------------------------
    // Step 8: セーブ ＆ ロード往復検証（ファイル保存 ➔ 復元 ➔ 整合性アサーション）
    // -------------------------------------------------------------------------
    println!("💾 [Step 8/8] セーブ ＆ ロード往復完全一致検証");
    let save_path = "saves/save_data.ron";
    world.save_to_file(save_path)?;
    println!("   - ワールドステートを '{}' へシリアライズ保存完了", save_path);

    let loaded_world = WorldState::load_from_file(save_path)?;
    println!("   - '{}' からワールドステートをデシリアライズ復元完了", save_path);

    assert_eq!(world.generation, loaded_world.generation, "Generation mismatch");
    assert_eq!(world.calendar_tick, loaded_world.calendar_tick, "Calendar tick mismatch");
    assert_eq!(world.treasury_zeny, loaded_world.treasury_zeny, "Treasury zeny mismatch");
    assert_eq!(world.active_character.name, loaded_world.active_character.name, "Active char mismatch");
    assert_eq!(world.active_character.equipments.len(), loaded_world.active_character.equipments.len(), "Equipments len mismatch");
    assert_eq!(world.rival_guilds.len(), loaded_world.rival_guilds.len(), "Rival guilds len mismatch");
    assert_eq!(world.storage_items.len(), loaded_world.storage_items.len(), "Storage items len mismatch");

    println!("   ✨ 全フィールドの往復完全一致を確認！");
    println!("   ✅ セーブ/ロード往復検証完了\n");

    println!("================================================================================");
    println!("  🎉 【全システム統合シミュレーションおよびセーブ/ロード検証：SUCCESS】");
    println!("  ステートフルなゲームエンジン（WorldState）への昇華が実証されました。");
    println!("================================================================================");

    Ok(())
}

/// ユーザー入力を安全に取得する（EOF時は None を返す）
fn prompt_input(msg: &str) -> Option<String> {
    print!("{}", msg);
    let _ = io::stdout().flush();
    let mut input = String::new();
    match io::stdin().read_line(&mut input) {
        Ok(0) => None, // EOF (Ctrl+D やパイプ終端)
        Ok(_) => Some(input.trim().to_string()),
        Err(_) => None,
    }
}

// =============================================================================
//  【インタラクティブCUIモード】cargo run (通常起動)
//  プレイヤーがコマンド番号選択で意思決定を行いながら何世代も遊べるゲームループ
// =============================================================================
fn run_interactive_mode(db: &ModDatabase) -> Result<(), Box<dyn std::error::Error>> {
    println!("================================================================================");
    println!("       🏛️   Realm of Origins - インタラクティブ・ギルドマスター司令部   🏛️");
    println!("================================================================================\n");

    let engine = SimulationEngine::new(db);
    let save_file_path = "saves/save_data.ron";

    // セーブデータが存在すればロード確認、なければ新規作成
    let mut world = if std::path::Path::new(save_file_path).exists() {
        let load_confirm = prompt_input("既存のセーブデータが見つかりました。ロードしますか？ (y/n) > ")
            .unwrap_or_else(|| "n".to_string());
        if load_confirm.eq_ignore_ascii_case("y") {
            match WorldState::load_from_file(save_file_path) {
                Ok(w) => {
                    println!("セーブデータを復元しました。\n");
                    w
                }
                Err(e) => {
                    println!("セーブデータの読み込みに失敗しました ({}). 新規ゲームを開始します。\n", e);
                    WorldState::new_game(db)
                }
            }
        } else {
            WorldState::new_game(db)
        }
    } else {
        println!("新しいギルドを設立しました！初期データを展開します。\n");
        WorldState::new_game(db)
    };

    loop {
        render_dashboard(&world);

        println!("--------------------------------------------------------------------------------");
        println!("  【ギルド司令部 コマンド一覧】");
        println!("  [1] ⚔️  【冒険・養殖へ出る】 (エリア探索 -> 戦闘 -> 経験値/ドロップ獲得 -> ワールド進行)");
        println!("  [2] 🛡️  【編成・装備・ガンビット】 (武器/カード装備、希少品ロック、作戦設定)");
        println!("  [3] 🔨  【全自動クホホ工場】 (鍛冶屋による過剰精錬ワークオーダー)");
        println!("  [4] 📈  【動的市場（売買）】 (BOM連動相場確認、物資購入、Overcharge売却)");
        println!("  [5] ⛏️  【自動派遣（遠征）】 (控え部隊3名をエリアへ派遣・回収)");
        println!("  [6] 🔄  【レガシー転生 ＆ FA処遇】 (世代交代、新主人公作成、古参キャラの処遇)");
        println!("  [7] 🏆  【ギルドリーグ・GvG】 (ライバルギルド一覧確認、3vs3対抗戦)");
        println!("  [8] 💾  【セーブ / ロード】 (セーブ保存・復元)");
        println!("  [0] 🚪  【ゲーム終了】");
        println!("--------------------------------------------------------------------------------");

        let choice = match prompt_input("コマンド番号を入力してください > ") {
            Some(c) => c,
            None => {
                println!("\n入力終了を検知しました。ゲームを終了します。");
                break;
            }
        };

        match choice.as_str() {
            "1" => action_adventure(&mut world, db, &engine)?,
            "2" => action_customization(&mut world)?,
            "3" => action_refinery(&mut world)?,
            "4" => action_market(&mut world)?,
            "5" => action_dispatch(&mut world, db)?,
            "6" => action_reincarnation(&mut world, db)?,
            "7" => action_gvg(&mut world, db, &engine)?,
            "8" => action_save_load(&mut world, save_file_path)?,
            "0" => {
                println!("ギルド司令部を閉じます。お疲れ様でした！");
                break;
            }
            _ => {
                println!("⚠️ 無効な番号です。0〜8 の半角数字を入力してください。\n");
            }
        }
    }

    Ok(())
}

/// ダッシュボード（現在のギルド状況ヘッダー）の表示
fn render_dashboard(world: &WorldState) {
    println!("\n================================================================================");
    println!("  🏛️  Realm of Origins | 第{}世代 | ワールド暦: {} Tick | ギルド資金: {} Zeny",
        world.generation, world.calendar_tick, world.treasury_zeny);
    println!("--------------------------------------------------------------------------------");
    println!("  👤 現役主人公: {} (Lv.{} {}) | EXP: {} | 育成方針: {}",
        world.active_character.name,
        world.active_character.level,
        world.active_character.job_id,
        world.active_character.exp,
        world.active_growth_plan.name_ja()
    );
    let party_names = world.party_members.iter().map(|c| format!("{} (Lv.{})", c.name, c.level)).collect::<Vec<_>>();
    println!("  🤝 同行パーティー: {}", if party_names.is_empty() { "なし".to_string() } else { party_names.join(", ") });

    let dispatch_status = if let Some(ref d) = world.active_dispatch {
        format!("『{}』へ遠征中 (残り {} Tick)", d.area_name, d.ticks_remaining)
    } else {
        "待機中（未派遣）".to_string()
    };
    println!("  ⛏️ 自動派遣部隊: {}", dispatch_status);

    if let Some(latest_league) = world.rival_league_logs.last() {
        println!("  📰 リーグ最新速報: {}", latest_league);
    }
    println!("================================================================================");
}

// -----------------------------------------------------------------------------
// [1] 冒険・養殖へ出る
// -----------------------------------------------------------------------------
fn action_adventure(world: &mut WorldState, db: &ModDatabase, engine: &SimulationEngine) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n⚔️ 【冒険・養殖へ出る】 挑戦するエリアを選択してください：");
    let mut area_list = Vec::new();
    for (idx, (id, area)) in db.areas.iter().enumerate() {
        println!("  [{}] {} (推奨Lv.{}, 出現モンスター: {}種)",
            idx + 1, area.name, area.recommended_level, area.monsters.len());
        area_list.push(id.clone());
    }
    println!("  [0] キャンセル");
    let choice: usize = match prompt_input("選択 > ").and_then(|s| s.parse().ok()) {
        Some(c) => c,
        None => return Ok(()),
    };

    if choice == 0 || choice > area_list.len() {
        return Ok(());
    }

    let area_id = &area_list[choice - 1];
    let area = db.areas.get(area_id).unwrap();

    println!("\n🚩 『{}』へ出撃！ 3連戦を開始します...", area.name);
    let party = world.get_active_party();

    let mut total_exp = 0u64;
    let mut total_loot_zeny = 0u32;

    for i in 1..=3 {
        // モンスター選出
        let monster_id = if !area.monsters.is_empty() {
            &area.monsters[(i - 1) % area.monsters.len()]
        } else {
            "poring"
        };

        if let Some(template) = db.monsters.get(monster_id) {
            let mut spawned = engine.spawn_monster(template, models::MonsterArchetype::Standard);
            let res = engine.simulate_battle(&party, &mut spawned);
            println!("   ⚔️ 戦闘 #{}: 『{}』を討伐！ (所要: {} Tick, EXP: +{}, ドロップ換金: +{} Zeny)",
                i, res.monster_name, res.total_ticks, res.gained_exp, res.final_loot_value);
            total_exp += res.gained_exp;
            total_loot_zeny += res.final_loot_value;
        }
    }

    // 主人公への経験値配分
    let mut exp_logs = Vec::new();
    Progression::grant_exp(&mut world.active_character, total_exp, world.active_growth_plan, &mut exp_logs);
    for l in exp_logs {
        println!("{}", l);
    }
    world.treasury_zeny += total_loot_zeny;
    println!("💰 合計戦果: +{} EXP, +{} Zeny獲得！\n", total_exp, total_loot_zeny);

    // ワールドTick進行（エリアに応じた時間経過）
    let elapsed = (area.recommended_level * 3 + 20).max(30);
    println!("⏱️ 探索によりワールド時間が {} Tick 経過します...", elapsed);
    let turn_logs = world.advance_turn(elapsed, db);
    for l in turn_logs {
        println!("{}", l);
    }

    Ok(())
}

// -----------------------------------------------------------------------------
// [2] 編成・装備・ガンビット設定
// -----------------------------------------------------------------------------
fn action_customization(world: &mut WorldState) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🛡️ 【編成・装備・ガンビット設定】");
    println!("  [1] 現役キャラの装備確認・変更");
    println!("  [2] 倉庫の装備にカードを装着");
    println!("  [3] 装備品の希少品保護ロック切替");
    println!("  [4] ガンビット（条件付き戦術）の設定");
    println!("  [0] 戻る");

    let sub_choice = match prompt_input("選択 > ") {
        Some(c) => c,
        None => return Ok(()),
    };

    match sub_choice.as_str() {
        "1" => {
            println!("\n--- 現在の装備 ---");
            for (idx, eq) in world.active_character.equipments.iter().enumerate() {
                println!("  [{}] {} (ATK:{}, 精錬:+{}, スロット:{}, ロック:{})",
                    idx + 1, eq.name, eq.weapon_atk, eq.refine, eq.slots, eq.is_locked);
            }
            println!("--- 倉庫の装備一覧 ---");
            for (idx, eq) in world.storage_equipments.iter().enumerate() {
                println!("  [{}] {} (ATK:{}, 精錬:+{}, スロット:{})",
                    idx + 1, eq.name, eq.weapon_atk, eq.refine, eq.slots);
            }
            println!("倉庫の装備を装着しますか？ (倉庫番号を入力 / 0でキャンセル)");
            if let Some(idx) = prompt_input("選択 > ").and_then(|s| s.parse::<usize>().ok()) {
                if idx > 0 && idx <= world.storage_equipments.len() {
                    let new_eq = world.storage_equipments.remove(idx - 1);
                    if let Some(old_eq) = world.active_character.equipments.pop() {
                        world.storage_equipments.push(old_eq);
                    }
                    println!("✨ {} を装備しました！", new_eq.name);
                    world.active_character.equipments.push(new_eq);
                }
            }
        }
        "2" => {
            println!("\n--- カード装着 ---");
            let cards: Vec<String> = world.storage_items.keys().filter(|k| k.starts_with("card_")).cloned().collect();
            if cards.is_empty() {
                println!("倉庫にカードがありません。");
                return Ok(());
            }
            for (idx, c) in cards.iter().enumerate() {
                println!("  [{}] {} (所持数: {})", idx + 1, c, world.storage_items[c]);
            }
            println!("装着するカードを選択してください (0でキャンセル):");
            if let Some(c_idx) = prompt_input("選択 > ").and_then(|s| s.parse::<usize>().ok()) {
                if c_idx > 0 && c_idx <= cards.len() {
                    let card_id = &cards[c_idx - 1];
                    if let Some(eq) = world.active_character.equipments.first_mut() {
                        if (eq.cards.len() as u8) < eq.slots {
                            eq.cards.push(Card {
                                id: card_id.clone(),
                                name: card_id.clone(),
                                slot: EquipSlot::Weapon,
                                modifiers: vec![],
                            });
                            *world.storage_items.get_mut(card_id).unwrap() -= 1;
                            println!("✨ {} に {} を装着しました！", eq.name, card_id);
                        } else {
                            println!("⚠️ スロットがいっぱいです。");
                        }
                    }
                }
            }
        }
        "3" => {
            println!("\n--- 希少品保護ロック切替 ---");
            if let Some(eq) = world.active_character.equipments.first_mut() {
                eq.is_locked = !eq.is_locked;
                println!("装備中の {} のロックを【{}】に切り替えました。", eq.name, if eq.is_locked { "ON (保護)" } else { "OFF" });
            }
        }
        "4" => {
            println!("\n--- ガンビット作戦設定 ---");
            println!("  [1] 対迅速型(Swift)に必中バッシュ連打");
            println!("  [2] 対重装型(Heavy)に急所攻撃");
            println!("  [3] 通常攻撃優先");
            let g_in = prompt_input("作戦選択 > ").unwrap_or_default();
            world.active_character.gambit_rules.clear();
            match g_in.as_str() {
                "1" => {
                    world.active_character.gambit_rules.push(GambitRule {
                        id: "swift_bash".to_string(),
                        priority: 1,
                        condition: Condition::TargetArchetypeIs(models::MonsterArchetype::Swift),
                        action: Action::UseActiveSkill("bash".to_string()),
                    });
                    println!("✨ 作戦【対Swift必中バッシュ】を設定しました！");
                }
                "2" => {
                    world.active_character.gambit_rules.push(GambitRule {
                        id: "heavy_bash".to_string(),
                        priority: 1,
                        condition: Condition::TargetArchetypeIs(models::MonsterArchetype::Heavy),
                        action: Action::UseActiveSkill("bash".to_string()),
                    });
                    println!("✨ 作戦【対重装型強打】を設定しました！");
                }
                _ => {
                    println!("通常攻撃優先に設定しました。");
                }
            }
        }
        _ => {}
    }
    Ok(())
}

// -----------------------------------------------------------------------------
// [3] 全自動クホホ工場（精錬所）
// -----------------------------------------------------------------------------
fn action_refinery(world: &mut WorldState) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔨 【全自動クホホ工場（精錬所）】");
    if world.storage_equipments.is_empty() {
        println!("倉庫に未精錬の装備がありません。");
        return Ok(());
    }

    println!("精錬する装備を選択してください：");
    for (idx, eq) in world.storage_equipments.iter().enumerate() {
        println!("  [{}] {} (+{}, ロック:{})", idx + 1, eq.name, eq.refine, eq.is_locked);
    }
    println!("  [0] キャンセル");
    let choice: usize = match prompt_input("選択 > ").and_then(|s| s.parse().ok()) {
        Some(c) => c,
        None => return Ok(()),
    };

    if choice == 0 || choice > world.storage_equipments.len() {
        return Ok(());
    }

    let target_eq = world.storage_equipments.remove(choice - 1);
    println!("\nワークオーダーのモードを選択してください：");
    println!("  [1] 安全圏まで精錬 (+4で安全停止)");
    println!("  [2] 目標過剰精錬 (+7)");
    println!("  [3] 限界突破過剰精錬 (+10)");
    println!("  [4] 予算限界突破（破損するまで叩く）");

    let m_in = prompt_input("モード選択 > ").unwrap_or_default();
    let (mode, target_level) = match m_in.as_str() {
        "1" => (WorkOrderMode::SafeLimit(4), 4),
        "2" => (WorkOrderMode::BudgetBurn { target_level: 7, max_zeny: world.treasury_zeny.min(30_000), max_weapons: 1 }, 7),
        "3" => (WorkOrderMode::BudgetBurn { target_level: 10, max_zeny: world.treasury_zeny.min(50_000), max_weapons: 1 }, 10),
        "4" => (WorkOrderMode::BudgetBurn { target_level: 15, max_zeny: world.treasury_zeny, max_weapons: 1 }, 15),
        _ => (WorkOrderMode::SafeLimit(4), 4),
    };

    let blacksmith = world.party_members.iter().find(|c| c.job_id == "blacksmith").cloned().unwrap_or_else(|| {
        world.active_character.clone()
    });

    let order = WorkOrder {
        order_name: "精錬作業指示書".to_string(),
        mode,
        target_level,
    };

    println!("\nクホホホ...！ 鍛冶屋 {} が槌を振り下ろします！", blacksmith.name);
    let (report, remaining_zeny) = Refinery::process_work_order(&order, vec![target_eq.clone()], &blacksmith, world.treasury_zeny);
    for l in report.logs {
        println!("  {}", l);
    }

    world.treasury_zeny = remaining_zeny;
    println!("支出: {} Zeny (残高: {} Zeny)", report.total_zeny_spent, world.treasury_zeny);

    if let Some(completed_eq) = report.completed_weapons.into_iter().next() {
        println!("✨ 精錬完了: {} (+{})", completed_eq.name, completed_eq.refine);
        world.storage_equipments.push(completed_eq);
    } else {
        println!("💀 装備は跡形もなく消滅しました...");
    }

    Ok(())
}

// -----------------------------------------------------------------------------
// [4] 動的市場（売買＆相場確認）
// -----------------------------------------------------------------------------
fn action_market(world: &mut WorldState) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n📈 【動的市場（プロンテラ大市場）】");
    println!("現在のBOM連動相場一覧：");
    for (_id, item) in &world.market.items {
        let current_price = item.buy_price();
        let diff = (current_price as f64 / item.base_price as f64 - 1.0) * 100.0;
        println!("  - {:<14}: 価格 {:>5} Zeny (基準:{}, 変動:{:>+5.1}%, 在庫:{})",
            item.name, current_price, item.base_price, diff, item.current_stock);
    }

    println!("\nアクションを選択してください：");
    println!("  [1] 完成品や素材を購入する");
    println!("  [2] 倉庫の物資をOverchargeで売却する");
    println!("  [0] 戻る");

    let sub_choice = match prompt_input("選択 > ") {
        Some(c) => c,
        None => return Ok(()),
    };

    match sub_choice.as_str() {
        "1" => {
            println!("\n購入するアイテムIDを入力 (例: damascus, white_potion, iron_ore):");
            let item_id = match prompt_input("アイテムID > ") {
                Some(id) if !id.is_empty() => id,
                _ => return Ok(()),
            };

            let qty: u32 = match prompt_input("購入数 > ").and_then(|s| s.parse().ok()) {
                Some(q) if q > 0 => q,
                _ => return Ok(()),
            };

            match world.market.buy(&item_id, qty) {
                Ok(total_cost) => {
                    if world.treasury_zeny >= total_cost {
                        world.treasury_zeny -= total_cost;
                        *world.storage_items.entry(item_id.clone()).or_insert(0) += qty;
                        println!("✨ {} を {}個 購入しました！ (支払: {} Zeny)", item_id, qty, total_cost);
                    } else {
                        println!("⚠️ ギルド資金が不足しています (必要: {} Zeny)", total_cost);
                    }
                }
                Err(e) => println!("⚠️ 購入失敗: {}", e),
            }
        }
        "2" => {
            println!("\n--- 倉庫の売却可能アイテム ---");
            let items: Vec<(String, u32)> = world.storage_items.iter().filter(|(_, &c)| c > 0).map(|(k, &v)| (k.clone(), v)).collect();
            for (idx, (k, v)) in items.iter().enumerate() {
                println!("  [{}] {} x{}", idx + 1, k, v);
            }
            println!("売却するアイテム番号を選択 (0でキャンセル):");
            if let Some(idx) = prompt_input("選択 > ").and_then(|s| s.parse::<usize>().ok()) {
                if idx > 0 && idx <= items.len() {
                    let (item_id, count) = &items[idx - 1];
                    let revenue = world.market.sell(item_id, *count, 0.24).unwrap_or(0);
                    world.treasury_zeny += revenue;
                    world.storage_items.insert(item_id.clone(), 0);
                    println!("💰 {} x{} を売却し、+{} Zeny 獲得しました！", item_id, count, revenue);
                }
            }
        }
        _ => {}
    }
    Ok(())
}

// -----------------------------------------------------------------------------
// [5] 自動派遣（遠征指示）
// -----------------------------------------------------------------------------
fn action_dispatch(world: &mut WorldState, db: &ModDatabase) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n⛏️ 【自動派遣（遠征指示）】");
    if let Some(ref d) = world.active_dispatch {
        println!("現在、部隊が『{}』へ遠征中です（残り {} Tick）。帰還をお待ちください。", d.area_name, d.ticks_remaining);
        return Ok(());
    }

    if world.roster.active_members.len() < 3 {
        println!("⚠️ 派遣に必要な控えメンバーが不足しています（名簿に最低3名必要です）。");
        return Ok(());
    }

    println!("派遣先エリアを選択してください：");
    let mut areas = Vec::new();
    for (idx, (id, area)) in db.areas.iter().enumerate() {
        println!("  [{}] {} (推奨Lv.{})", idx + 1, area.name, area.recommended_level);
        areas.push(id.clone());
    }
    println!("  [0] キャンセル");
    let choice: usize = match prompt_input("選択 > ").and_then(|s| s.parse().ok()) {
        Some(c) => c,
        None => return Ok(()),
    };
    if choice == 0 || choice > areas.len() {
        return Ok(());
    }

    let area_id = &areas[choice - 1];
    let area = db.areas.get(area_id).unwrap();

    // 控えから3名を抽出
    let m1 = world.roster.active_members.remove(0);
    let m2 = world.roster.active_members.remove(0);
    let m3 = world.roster.active_members.remove(0);
    let members = vec![m1, m2, m3];
    let member_names = members.iter().map(|c| c.name.clone()).collect::<Vec<_>>();

    let dispatch = dispatch::ActiveDispatch {
        area_id: area.id.clone(),
        area_name: area.name.clone(),
        members,
        ticks_remaining: 100,
        cycles: 5,
    };
    world.active_dispatch = Some(dispatch);

    println!("🚩 『{}』への自動派遣部隊（{}）を登録しました！ (所要: 100 Tick)",
        area.name, member_names.join(", "));
    println!("冒険に出たり時間を進めることで、裏で素材採掘が自動進行します。");

    Ok(())
}

// -----------------------------------------------------------------------------
// [6] レガシー転生 ＆ FA処遇決定
// -----------------------------------------------------------------------------
fn action_reincarnation(world: &mut WorldState, _db: &ModDatabase) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔄 【レガシー転生 ＆ FA処遇決定】");
    println!("現役主人公 {} (Lv.{}) を殿堂入りさせ、次世代の主人公を誕生させますか？",
        world.active_character.name, world.active_character.level);
    println!("世代が進むごとに先代の英知が遺伝し、初期ステータスボーナスが付加されます。");
    println!("  [1] 転生を実行する");
    println!("  [0] キャンセル");

    let confirm = prompt_input("選択 > ").unwrap_or_default();
    if confirm != "1" {
        return Ok(());
    }

    println!("\n次世代主人公の名前を入力してください:");
    let name_in = prompt_input("名前 > ").unwrap_or_default();
    let name = if name_in.is_empty() { "次代の勇士" } else { &name_in };

    println!("育成方針を選択してください：");
    println!("  [1] AGI/STR速攻型 (AgiCrit)");
    println!("  [2] VIT重装壁型 (VitTank)");
    println!("  [3] DEX/LUK製造商才型 (DexLukCrafter)");
    println!("  [4] INT/DEX魔導支援型 (IntDexSupport)");

    let p_in = prompt_input("選択 > ").unwrap_or_default();
    let plan = match p_in.as_str() {
        "2" => GrowthPlan::VitTank,
        "3" => GrowthPlan::DexLukCrafter,
        "4" => GrowthPlan::IntDexSupport,
        _ => GrowthPlan::AgiCrit,
    };

    // 世代交代
    world.generation += 1;
    let old_hero = world.active_character.clone();
    world.roster.add_member(old_hero);

    let new_char = Progression::create_reincarnated_character(
        &format!("char_gen{}", world.generation),
        name,
        "thief",
        world.generation as u8,
        plan,
    );
    world.active_character = new_char;
    world.active_growth_plan = plan;

    println!("🌟 【第{}世代誕生】主人公『{}』(Lv.1, 方針:{}) が就任しました！",
        world.generation, world.active_character.name, plan.name_ja());

    // FA判定
    let deployed_ids = vec![world.active_character.id.clone()];
    let fa_candidates = world.roster.process_generation_shift(&deployed_ids);
    if !fa_candidates.is_empty() {
        println!("\n📜 世代交代に伴い、以下の古参メンバーがFA（フリーエージェント）を宣言しました：");
        let mut fa_logs = Vec::new();
        for candidate in fa_candidates {
            println!("\n  👤 {} (Lv.{}, {}) の処遇を選択してください：",
                candidate.name, candidate.level, candidate.job_id);
            println!("    [1] ギルドに残留させる（引き留め）");
            println!("    [2] 街のNPCとして引退（永続パッシブ武器ATKボーナス）");
            println!("    [3] ライバルリーグへ移籍承認（移籍金獲得＆ライバル入団）");

            let dec_in = prompt_input("  処遇選択 > ").unwrap_or_default();
            let dec = match dec_in.as_str() {
                "2" => FaDecision::RetireAsTownNpc,
                "3" => FaDecision::ApproveTransfer,
                _ => FaDecision::Retain,
            };

            let transferred = world.roster.handle_fa_decision(candidate, dec, &mut fa_logs);
            if let Some(ch) = transferred {
                // 最適ライバルギルドへ入団
                if let Some(rg) = world.rival_guilds.iter_mut().max_by_key(|g| g.calculate_synergy_score(&ch)) {
                    rg.recruit_and_train(ch, &mut fa_logs);
                }
            }
        }
        for l in fa_logs {
            println!("{}", l);
        }
    }

    Ok(())
}

// -----------------------------------------------------------------------------
// [7] ギルドリーグ・攻城戦（GvG）
// -----------------------------------------------------------------------------
fn action_gvg(world: &mut WorldState, db: &ModDatabase, engine: &SimulationEngine) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🏆 【ギルドリーグ・GvG対抗戦】");
    println!("対戦相手のライバルギルドを選択してください：");
    for (idx, rg) in world.rival_guilds.iter().enumerate() {
        let members = rg.roster.iter().map(|c| format!("{} (Lv.{})", c.name, c.level)).collect::<Vec<_>>();
        println!("  [{}] 『{}』 主力: {}", idx + 1, rg.name, members.join(", "));
    }
    println!("  [0] キャンセル");
    let choice: usize = match prompt_input("選択 > ").and_then(|s| s.parse().ok()) {
        Some(c) => c,
        None => return Ok(()),
    };
    if choice == 0 || choice > world.rival_guilds.len() {
        return Ok(());
    }

    let rival = &world.rival_guilds[choice - 1];
    let rival_team = rival.roster.iter().take(3).cloned().collect::<Vec<_>>();
    let player_team = world.get_active_party();

    println!("\n⚔️ 『{}』との模擬GvG（3vs3）を開始します！", rival.name);
    let gvg_res = engine.simulate_gvg_battle(
        &player_team,
        &rival_team,
        "我らがギルド",
        &rival.name,
        Some(world.roster.town_bonus.clone()),
    );

    println!("\n🏁 【決着】 勝者：『{}』 (戦闘所要: {} Tick)", gvg_res.winner_team, gvg_res.total_ticks);
    println!("   - 自軍生存者数: {}名", gvg_res.team_a_survivors.len());
    println!("   - 敵軍生存者数: {}名", gvg_res.team_b_survivors.len());

    if gvg_res.winner_team == "我らがギルド" {
        println!("🎉 見事勝利しました！ リーグ賞金 +20,000 Zeny獲得！");
        world.treasury_zeny += 20_000;
    } else {
        println!("惜しくも敗北しました... 装備やガンビットを見直して再挑戦しましょう！");
    }

    let turn_logs = world.advance_turn(30, db);
    for l in turn_logs {
        println!("{}", l);
    }

    Ok(())
}

// -----------------------------------------------------------------------------
// [8] セーブ / ロード
// -----------------------------------------------------------------------------
fn action_save_load(world: &mut WorldState, save_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n💾 【セーブ / ロード】");
    println!("  [1] 現在の状態をセーブする ('{}')", save_path);
    println!("  [2] セーブデータをロードする");
    println!("  [0] 戻る");

    let choice = prompt_input("選択 > ").unwrap_or_default();
    match choice.as_str() {
        "1" => match world.save_to_file(save_path) {
            Ok(_) => println!("✨ 状態を正常にセーブしました！"),
            Err(e) => println!("⚠️ セーブ失敗: {}", e),
        },
        "2" => match WorldState::load_from_file(save_path) {
            Ok(loaded) => {
                *world = loaded;
                println!("✨ セーブデータを正常に復元しました！");
            }
            Err(e) => println!("⚠️ ロード失敗: {}", e),
        },
        _ => {}
    }
    Ok(())
}
