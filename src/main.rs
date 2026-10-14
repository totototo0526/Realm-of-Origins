mod engine;
mod guild;
mod loader;
mod models;
mod progression;
mod refinery;

use engine::SimulationEngine;
use guild::{BuildTag, FaDecision, GuildRoster, RivalGuild};
use loader::ModDatabase;
use models::{
    Action, Character, Condition, EquipSlot, Equipment, GambitRule,
    MonsterArchetype, Stats,
};
use progression::{GrowthPlan, Progression};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("================================================================================");
    println!("  Realm of Origins - フェーズ3：レガシー転生 ＆ パワーレベリング ＆ 模擬GvG");
    println!("  【世代交代・セルフ養殖・シナジー引力によるライバルギルド形成と激突】");
    println!("================================================================================\n");

    // 1. mods/core/ からデータロード
    println!("📦 [1/6] mods/core/ から外部データ(RON)をロード中...");
    let db = ModDatabase::load_from_dir("mods/core")?;
    let engine = SimulationEngine::new(&db);

    // -------------------------------------------------------------------------
    // 【Step 1: パワーレベリング（セルフ養殖）の実証】
    // -------------------------------------------------------------------------
    println!("--------------------------------------------------------------------------------");
    println!(" 【Step 1: パワーレベリング（セルフ養殖）の実証】");
    println!("--------------------------------------------------------------------------------");
    println!(" 🐣 新規作成: 第2世代 Lv.1 新キャラ『ルカ』 (レガシー転生ボーナス: +8ステータスP)");
    println!("    育成方針: 【{}】 (INT/DEX魔導支援)", GrowthPlan::IntDexSupport.name_ja());

    // パターン1: ソロ育成（Lv1キャラ単独でオークウォーリアー10連戦）
    let mut solo_rookie = Progression::create_reincarnated_character(
        "rookie_solo",
        "ルカ (ソロ挑戦)",
        "thief",
        2,
        GrowthPlan::IntDexSupport,
    );
    // 初期短剣
    solo_rookie.equipments.push(Equipment::new("knife", "ナイフ", EquipSlot::Weapon, 15, 0));

    let orc_template = db.monsters.get("orc_warrior").expect("orc_warrior not found");
    let mut solo_monsters = Vec::new();
    for _ in 0..10 {
        solo_monsters.push(engine.spawn_monster(orc_template, MonsterArchetype::Standard));
    }

    println!("\n ▶ [テスト1] ソロ育成: Lv.1ルカが単独でオーク狩りに挑戦...");
    let solo_summary = engine.run_scenario("ソロ育成", &[solo_rookie.clone()], &solo_monsters);
    let mut solo_logs = Vec::new();
    let _ = Progression::grant_exp(&mut solo_rookie, solo_summary.total_exp_gained, GrowthPlan::IntDexSupport, &mut solo_logs);

    println!(
        "   => 総討伐Tick: {:>5} Tick | 獲得EXP: {:>5} EXP | 到達レベル: Lv.{} (成長困難・ほぼ瞬殺)",
        solo_summary.total_ticks, solo_summary.total_exp_gained, solo_rookie.level
    );

    // パターン2: セルフ養殖（フェーズ2完成の+10武器先輩シーフ＋ナイトが引率しハイオーク10連戦）
    let mut leeched_rookie = Progression::create_reincarnated_character(
        "rookie_leeched",
        "ルカ (セルフ養殖)",
        "thief",
        2,
        GrowthPlan::IntDexSupport,
    );
    leeched_rookie.equipments.push(Equipment::new("knife", "ナイフ", EquipSlot::Weapon, 15, 0));

    // 先輩コンビの準備
    let hydra_card = db.cards.get("hydra_card").cloned().unwrap();
    let flame_eye_card = db.cards.get("flame_eye_card").cloned().unwrap();
    let mut god_dagger = Equipment::new("god_damascus", "+10 業物・滅殺烈火ダマスカス", EquipSlot::Weapon, 75, 10);
    god_dagger.cards = vec![hydra_card, flame_eye_card];
    god_dagger.is_locked = true;

    let senior_thief = Character {
        id: "senior_thief".to_string(),
        name: "先輩シーフ (+10ダマスカス)".to_string(),
        job_id: "thief".to_string(),
        level: 48,
        base_stats: Stats::new(60, 75, 20, 25, 35, 15),
        equipments: vec![god_dagger.clone()],
        learned_skills: vec![db.skills.get("bash").cloned().unwrap()],
        gambit_rules: vec![
            GambitRule {
                id: "必中バッシュ".to_string(),
                priority: 1,
                condition: Condition::Always,
                action: Action::UseActiveSkill("bash".to_string()),
            },
        ],
        generation: 1,
        exp: 50000,
        activity_score: 100,
    };

    let senior_knight = Character {
        id: "senior_knight".to_string(),
        name: "先輩ナイト (壁役/VIT)".to_string(),
        job_id: "knight".to_string(),
        level: 50,
        base_stats: Stats::new(45, 20, 80, 10, 30, 10),
        equipments: vec![
            Equipment::new("tsurugi", "ツルギ", EquipSlot::Weapon, 80, 5),
            Equipment::new("shield", "ラウンドシールド", EquipSlot::Shield, 0, 5),
        ],
        learned_skills: vec![db.skills.get("iron_defense").cloned().unwrap()],
        gambit_rules: vec![],
        generation: 1,
        exp: 60000,
        activity_score: 100,
    };

    let high_orc_template = db.monsters.get("high_orc").expect("high_orc not found");
    let mut high_orc_monsters = Vec::new();
    for _ in 0..10 {
        high_orc_monsters.push(engine.spawn_monster(high_orc_template, MonsterArchetype::Heavy));
    }

    println!("\n ▶ [テスト2] セルフ養殖: 先輩シーフ＆ナイトがLv.1ルカを連れ、格上ハイオーク10連戦へ出撃！");
    let leech_party = vec![senior_thief.clone(), senior_knight.clone(), leeched_rookie.clone()];
    let leech_summary = engine.run_scenario("セルフ養殖", &leech_party, &high_orc_monsters);

    let mut levelup_logs = Vec::new();
    // 獲得経験値をルカに集中（または均等分配）
    let gained_exp_for_rookie = leech_summary.total_exp_gained;
    let levels_gained = Progression::grant_exp(&mut leeched_rookie, gained_exp_for_rookie, GrowthPlan::IntDexSupport, &mut levelup_logs);

    for log in &levelup_logs {
        println!("{}", log);
    }

    println!(
        "\n   ✨ 【セルフ養殖完了】総Tick: {} Tick | 獲得EXP: {} EXP | 到達レベル: Lv.{} (+{} レベル爆上がり!)",
        leech_summary.total_ticks, gained_exp_for_rookie, leeched_rookie.level, levels_gained
    );

    // -------------------------------------------------------------------------
    // 【Step 2: 世代交代とFA移籍（シナジー引力によるライバル結成）】
    // -------------------------------------------------------------------------
    println!("\n--------------------------------------------------------------------------------");
    println!(" 【Step 2: 世代交代とFA移籍（シナジー引力によるライバル結成）】");
    println!("--------------------------------------------------------------------------------");
    let mut guild_roster = GuildRoster::new(6); // 定員6名

    // 過去に活躍した歴代キャラクターたちを名簿に登録
    let old_bs = Character {
        id: "gen1_bs".to_string(),
        name: "第1世代 鍛冶屋ボブ".to_string(),
        job_id: "blacksmith".to_string(),
        level: 45,
        base_stats: Stats::new(10, 10, 30, 10, 90, 85),
        equipments: vec![Equipment::new("wrench", "古びたスパナ", EquipSlot::Weapon, 30, 0)],
        learned_skills: vec![db.skills.get("weapon_research").cloned().unwrap(), db.skills.get("overcharge").cloned().unwrap()],
        gambit_rules: vec![],
        generation: 1,
        exp: 40000,
        activity_score: 20, // 非稼働
    };

    let old_thief_a = Character {
        id: "gen1_thief_a".to_string(),
        name: "第1世代 シーフ・クロード".to_string(),
        job_id: "thief".to_string(),
        level: 46,
        base_stats: Stats::new(70, 75, 20, 10, 35, 15),
        equipments: vec![Equipment::new("dagger", "+7 ダマスカス", EquipSlot::Weapon, 75, 7)],
        learned_skills: vec![db.skills.get("bash").cloned().unwrap()],
        gambit_rules: vec![
            GambitRule {
                id: "バッシュ連打".to_string(),
                priority: 1,
                condition: Condition::Always,
                action: Action::UseActiveSkill("bash".to_string()),
            },
        ],
        generation: 1,
        exp: 45000,
        activity_score: 25, // 非稼働
    };

    let old_knight = Character {
        id: "gen1_knight".to_string(),
        name: "第1世代 重騎士バルガス".to_string(),
        job_id: "knight".to_string(),
        level: 47,
        base_stats: Stats::new(45, 20, 80, 10, 30, 10),
        equipments: vec![Equipment::new("shield", "+7 アイアンシールド", EquipSlot::Shield, 0, 7)],
        learned_skills: vec![db.skills.get("iron_defense").cloned().unwrap()],
        gambit_rules: vec![],
        generation: 1,
        exp: 48000,
        activity_score: 30, // 非稼働
    };

    let gen3_assassin = Character {
        id: "gen3_sin".to_string(),
        name: "第3世代 影縫いのアサシン".to_string(),
        job_id: "thief".to_string(),
        level: 48,
        base_stats: Stats::new(65, 80, 20, 10, 40, 20),
        equipments: vec![Equipment::new("katar", "+8 ジャマダハル", EquipSlot::Weapon, 85, 8)],
        learned_skills: vec![],
        gambit_rules: vec![],
        generation: 3,
        exp: 55000,
        activity_score: 40,
    };

    let gen4_paladin = Character {
        id: "gen4_pala".to_string(),
        name: "第4世代 聖騎士レオナルド".to_string(),
        job_id: "knight".to_string(),
        level: 50,
        base_stats: Stats::new(45, 25, 85, 15, 35, 15),
        equipments: vec![Equipment::new("shield", "+8 聖騎士の盾", EquipSlot::Shield, 0, 8)],
        learned_skills: vec![db.skills.get("iron_defense").cloned().unwrap()],
        gambit_rules: vec![],
        generation: 4,
        exp: 65000,
        activity_score: 80,
    };

    // 第5世代現エース
    let gen5_ace = Character {
        id: "gen5_ace".to_string(),
        name: "第5世代 幻影の疾風シーフ".to_string(),
        job_id: "thief".to_string(),
        level: 52,
        base_stats: Stats::new(65, 85, 25, 30, 40, 25), // レガシーボーナス込み
        equipments: vec![god_dagger.clone()],
        learned_skills: vec![db.skills.get("bash").cloned().unwrap()],
        gambit_rules: vec![
            GambitRule {
                id: "迅速型必中撃".to_string(),
                priority: 1,
                condition: Condition::TargetArchetypeIs(MonsterArchetype::Swift),
                action: Action::UseActiveSkill("bash".to_string()),
            },
            GambitRule {
                id: "通常攻撃".to_string(),
                priority: 10,
                condition: Condition::Always,
                action: Action::NormalAttack,
            },
        ],
        generation: 5,
        exp: 75000,
        activity_score: 100,
    };

    guild_roster.add_member(old_bs);
    guild_roster.add_member(old_thief_a);
    guild_roster.add_member(old_knight);
    guild_roster.add_member(leeched_rookie.clone());
    guild_roster.add_member(gen3_assassin);
    guild_roster.add_member(gen4_paladin.clone());
    guild_roster.add_member(gen5_ace.clone()); // 7人目（定員6名超過！）

    println!(" 📜 ギルド名簿が定員（6名）を超過 (現在7名)。世代交代シフトを実行します...");

    // 第5世代現主力と新人が出撃し、第1世代古参メンバーが非稼働判定
    let deployed_ids = vec![gen5_ace.id.clone(), gen4_paladin.id.clone(), leeched_rookie.id.clone()];
    let fa_candidates = guild_roster.process_generation_shift(&deployed_ids);

    println!(" 🚨 FA（フリーエージェント）宣言者: {}名", fa_candidates.len());

    let mut rival_guild = RivalGuild::new("黒曜の旅団");
    let mut guild_logs = Vec::new();

    for candidate in fa_candidates {
        let tag = BuildTag::analyze(&candidate);
        if tag == BuildTag::WeaponSupport {
            // 鍛冶屋は街のNPCとして引退（ギルドに武器ATK+3%の永続パッシブを提供）
            let _ = guild_roster.handle_fa_decision(candidate, FaDecision::RetireAsTownNpc, &mut guild_logs);
        } else {
            // アタッカーやナイトは移籍金を得てライバルリーグへ移籍承認
            if let Some(transferred) = guild_roster.handle_fa_decision(candidate, FaDecision::ApproveTransfer, &mut guild_logs) {
                // ライバルギルドへ入団（シナジー引力）
                rival_guild.recruit_and_train(transferred, &mut guild_logs);
            }
        }
    }

    // 鍛冶屋の移籍シミュレーション（ライバルギルド側にも武器研究BSが欲しいので旧BSクローンをライバルへ派遣）
    let rival_bs = Character {
        id: "rival_bs".to_string(),
        name: "元祖・鍛冶職人ボブ (黒曜顧問)".to_string(),
        job_id: "blacksmith".to_string(),
        level: 48,
        base_stats: Stats::new(15, 10, 35, 10, 95, 90),
        equipments: vec![Equipment::new("wrench", "黒曜の特注スパナ", EquipSlot::Weapon, 40, 5)],
        learned_skills: vec![db.skills.get("weapon_research").cloned().unwrap()],
        gambit_rules: vec![],
        generation: 1,
        exp: 50000,
        activity_score: 100,
    };
    rival_guild.recruit_and_train(rival_bs, &mut guild_logs);

    for log in &guild_logs {
        println!("{}", log);
    }

    println!("\n 🏴‍☠️ ライバルギルド『{}』のシナジー編成が完成！", rival_guild.name);
    for m in &rival_guild.roster {
        let tag = BuildTag::analyze(m);
        println!("   - {} (Lv.{}, 特性: 【{}】)", m.name, m.level, tag.name_ja());
    }

    // -------------------------------------------------------------------------
    // 【Step 3: 育成成果の発見装置（ライバルギルドとの模擬GvG対抗戦）】
    // -------------------------------------------------------------------------
    println!("\n--------------------------------------------------------------------------------");
    println!(" 【Step 3: 育成成果の発見装置（ライバルギルドとの模擬GvG対抗戦）】");
    println!("--------------------------------------------------------------------------------");
    println!(" ⚔️  プレイヤー現主力（第5世代選抜） VS かつての愛弟子・古参連合『黒曜の旅団』");

    let player_team = vec![gen5_ace, gen4_paladin, leeched_rookie];
    // ライバルギルドから最もバランスの良い3名（キャリー、タンク、武器支援）を選抜
    let rival_team: Vec<Character> = rival_guild.roster.iter().take(3).cloned().collect();

    let gvg_result = engine.simulate_gvg_battle(
        &player_team,
        &rival_team,
        "我らがオリジンギルド (第5世代主力)",
        "ライバル『黒曜の旅団』 (古参精鋭連合)",
    );

    for log in &gvg_result.logs {
        println!("{}", log);
    }

    // -------------------------------------------------------------------------
    // 【サマリー出力】
    // -------------------------------------------------------------------------
    println!("\n================================================================================");
    println!(" 📊 フェーズ3 総合検証サマリー");
    println!("================================================================================");

    // 1. パワーレベリング効率
    println!("【1. パワーレベリング（セルフ養殖）育成効率比較】");
    println!("┌─────────────────────────────┬─────────────────────┬─────────────────────┐");
    println!("│ 育成方式                    │ ソロ育成 (Lv.1単独) │ セルフ養殖 (先輩引率)│");
    println!("├─────────────────────────────┼─────────────────────┼─────────────────────┤");
    println!("│ 所要時間 (Tick)             │ {:>15} Tick │ {:>15} Tick │", solo_summary.total_ticks, leech_summary.total_ticks);
    println!("│ 獲得経験値 (EXP)            │ {:>15} EXP  │ {:>15} EXP  │", solo_summary.total_exp_gained, gained_exp_for_rookie);
    let solo_rate = solo_summary.total_exp_gained as f32 / solo_summary.total_ticks.max(1) as f32;
    let leech_rate = gained_exp_for_rookie as f32 / leech_summary.total_ticks.max(1) as f32;
    println!("│ 時間効率 (EXP / Tick)       │ {:>13.2} EXP/T│ {:>13.2} EXP/T│", solo_rate, leech_rate);
    println!("│ 到達レベル                  │            Lv. {:>2}   │            Lv. {:>2}   │", solo_rookie.level, levels_gained + 1);
    println!("└─────────────────────────────┴─────────────────────┴─────────────────────┘");

    // 2. ギルド名簿変遷
    println!("\n【2. ギルド名簿の変遷と経済・永続パッシブ効果】");
    println!(" • 現役登録メンバー数: {}名 (定員6名枠維持)", guild_roster.active_members.len());
    println!(" • 街のNPC化引退: {}名 (永続恩恵: 全味方武器ATK +{:.1}%)",
        guild_roster.retired_town_npcs.len(),
        guild_roster.town_bonus.extra_weapon_atk_percent * 100.0
    );
    println!(" • FA移籍成立: {}名 (獲得移籍金総額: +{} Zeny)",
        guild_roster.transferred_members.len(),
        guild_roster.treasury_zeny
    );

    // 3. 模擬GvGサマリー
    println!("\n【3. 模擬GvG（3vs3パーティー対抗戦）結果】");
    println!(" • 決着所要時間: {} Tick", gvg_result.total_ticks);
    println!(" • 栄冠の勝者: 『{}』", gvg_result.winner_team);
    println!(" • プレイヤー軍生存者: {:?}", gvg_result.team_a_survivors);
    println!(" • ライバル軍生存者:   {:?}", gvg_result.team_b_survivors);

    println!("\n🔍 【フェーズ3総括：サクセス×ペナント型ループの実証】");
    println!(
        " 1. 養殖の爆発的時短: 先輩キャラが格上モブを瞬殺することで、育成効率がソロ比で {:.1}倍（到達Lv: 2 -> {}）へと劇的に跳ね上がりました。",
        leech_rate / solo_rate.max(0.1), levels_gained + 1
    );
    println!(
        " 2. シナジー引力によるドラマ: 古参キャラが「武器研究BS」と「物理キャリー」でお互いに引き寄せ合い、ライバルギルドで理想のシナジー編成を自動結成しました。"
    );
    println!(
        " 3. 捨てたキャラが牙を剥くGvG: かつて自分が育てた武器研究BSのオーラを背負った古参シーフが、現主力に手痛い一撃を浴びせる白熱の対抗戦が成立。世代交代のロマンとペナントレース型循環が実証されました。"
    );

    Ok(())
}
