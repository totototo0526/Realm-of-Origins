use std::f32::consts::PI;
use macroquad::prelude::*;

use crate::engine::{BattleVisualEvent, Combatant, SimulationEngine, SpatialBattleSession};
use crate::hex_grid::{HexCoord, HexMap};
use crate::loader::ModDatabase;
use crate::models::{Character, FormationRole, MonsterArchetype};
use crate::progression::{GrowthPlan, Progression};
use crate::refinery::Refinery;
use crate::world::WorldState;

/// ダメージやMISS等の浮遊テキスト
#[derive(Debug, Clone)]
struct FloatingText {
    text: String,
    x: f32,
    y: f32,
    color: Color,
    lifetime: f32,
    max_lifetime: f32,
    is_big: bool,
}

/// 攻撃光線や範囲爆発などの視覚エフェクト
#[derive(Debug, Clone)]
enum EffectKind {
    LaserLine { color: Color },
    AoEBlast { radius_hexes: u32, color: Color },
    Slash { color: Color },
}

#[derive(Debug, Clone)]
struct VisualEffect {
    from_x: f32,
    from_y: f32,
    to_x: f32,
    to_y: f32,
    kind: EffectKind,
    lifetime: f32,
    max_lifetime: f32,
}

/// 再生速度コントロール
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlaySpeed {
    Paused,
    Normal,  // 1x (約0.08s / Tick)
    Fast,    // 3x (約0.025s / Tick)
    Instant, // 毎フレーム最大20 Ticks進行
}

/// 日本語フォントローダー（システム内フォント自動探索）
pub fn load_japanese_font() -> Option<Font> {
    let candidate_paths = [
        "/usr/share/fonts/opentype/ipafont-gothic/ipag.ttf",
        "/usr/share/fonts/truetype/fonts-japanese-gothic.ttf",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "C:/Windows/Fonts/msgothic.ttc",
        "C:/Windows/Fonts/meiryo.ttc",
    ];

    for path in candidate_paths {
        if let Ok(bytes) = std::fs::read(path) {
            if let Ok(font) = load_ttf_font_from_bytes(&bytes) {
                println!("Loaded Japanese font from: {}", path);
                return Some(font);
            }
        }
    }
    println!("Warning: No Japanese font found, fallback to default font.");
    None
}

/// GUI全体の管理状態
pub struct GuiApp {
    pub db: ModDatabase,
    pub world: WorldState,
    pub session: Option<SpatialBattleSession>,
    pub font: Option<Font>,
    pub play_speed: PlaySpeed,
    pub tick_accumulator: f32,
    pub chat_logs: Vec<String>,
    floating_texts: Vec<FloatingText>,
    visual_effects: Vec<VisualEffect>,
    pub selected_unit: Option<(bool, usize)>, // (is_team_a, index)
    pub refine_popup: Option<(String, bool, f32)>, // (msg, is_success, timer)
    pub origin_x: f32,
    pub origin_y: f32,
    pub hex_size: f32,
}

impl GuiApp {
    pub fn new(db: ModDatabase) -> Self {
        let engine = SimulationEngine::new(&db);
        let world = WorldState::new_game(&db);
        let font = load_japanese_font();

        let mut chat_logs = Vec::new();
        chat_logs.push("📜 『Realm of Origins』ギルド司令部へようこそ。".to_string());
        chat_logs.push("⚔️ 右側パネルから「隘路防衛＆養殖」や「空間GvG模擬戦」を開始してください。".to_string());
        chat_logs.push(format!("💰 現在のギルド資金: {} Zeny / 世代: 第{}世代", world.treasury_zeny, world.generation));

        // 初期状態で監視塔の隘路防衛バトルを展開しておく
        let chokepoint_map = HexMap::create_chokepoint_map();
        let mut party = vec![world.active_character.clone()];
        party.extend(world.party_members.clone());

        let template_high_orc = db.monsters.get("high_orc").expect("high_orc exists");
        let template_orc = db.monsters.get("orc_warrior").expect("orc_warrior exists");
        let m1 = engine.spawn_monster(template_high_orc, MonsterArchetype::Heavy);
        let m2 = engine.spawn_monster(template_orc, MonsterArchetype::Swift);
        let monsters = vec![m1, m2];
        let session = Some(engine.create_chokepoint_session(chokepoint_map, &party, &monsters));

        Self {
            db,
            world,
            session,
            font,
            play_speed: PlaySpeed::Normal,
            tick_accumulator: 0.0,
            chat_logs,
            floating_texts: Vec::new(),
            visual_effects: Vec::new(),
            selected_unit: None,
            refine_popup: None,
            origin_x: 410.0,
            origin_y: 270.0,
            hex_size: 28.0,
        }
    }

    /// 現在の出撃パーティーを取得
    pub fn get_player_party(&self) -> Vec<Character> {
        let mut party = vec![self.world.active_character.clone()];
        party.extend(self.world.party_members.clone());
        party
    }

    /// ヘクス座標から2.5Dクォータービュー画面ピクセル座標へ変換
    pub fn hex_to_screen(&self, coord: HexCoord) -> (f32, f32) {
        let (rx, ry) = HexCoord::hex_to_pixel(coord, self.hex_size);
        (self.origin_x + rx, self.origin_y + ry)
    }

    /// 画面ピクセル座標からヘクス座標へ逆変換
    pub fn screen_to_hex(&self, sx: f32, sy: f32) -> HexCoord {
        HexCoord::pixel_to_hex(sx - self.origin_x, sy - self.origin_y, self.hex_size)
    }

    /// テキスト描画ヘルパー
    fn draw_text_helper(&self, text: &str, x: f32, y: f32, font_size: f32, color: Color) {
        if let Some(ref f) = self.font {
            draw_text_ex(
                text,
                x,
                y,
                TextParams {
                    font: Some(f),
                    font_size: font_size as u16,
                    color,
                    ..Default::default()
                },
            );
        } else {
            draw_text(text, x, y, font_size, color);
        }
    }

    /// カスタムボタン描画（ホバー判定＆クリック検知）
    fn draw_button(&self, rect: Rect, label: &str, is_active: bool, base_color: Color) -> bool {
        let mouse_pos = mouse_position();
        let is_hover = rect.contains(Vec2::new(mouse_pos.0, mouse_pos.1));
        let clicked = is_hover && is_mouse_button_pressed(MouseButton::Left);

        let bg_color = if is_active {
            Color::new(0.2, 0.6, 0.9, 0.95)
        } else if is_hover {
            Color::new(base_color.r * 1.3, base_color.g * 1.3, base_color.b * 1.3, 0.95)
        } else {
            base_color
        };

        draw_rectangle(rect.x, rect.y, rect.w, rect.h, bg_color);
        draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1.5, if is_active { WHITE } else { Color::new(0.6, 0.7, 0.8, 0.8) });

        let font_size = 15.0;
        let text_x = rect.x + 10.0;
        let text_y = rect.y + rect.h * 0.65;
        self.draw_text_helper(label, text_x, text_y, font_size, if is_active { WHITE } else { Color::new(0.9, 0.9, 0.95, 1.0) });

        clicked
    }

    /// 1フレームごとの更新（戦闘ステップ実行、エフェクト更新）
    pub fn update(&mut self, dt: f32) {
        // 精錬ポップアップのタイマー
        if let Some((_, _, ref mut timer)) = self.refine_popup {
            *timer -= dt;
            if *timer <= 0.0 {
                self.refine_popup = None;
            }
        }

        // フローティングテキストの更新
        for ft in &mut self.floating_texts {
            ft.y -= 28.0 * dt;
            ft.lifetime -= dt;
        }
        self.floating_texts.retain(|ft| ft.lifetime > 0.0);

        // ビジュアルエフェクトの更新
        for ve in &mut self.visual_effects {
            ve.lifetime -= dt;
        }
        self.visual_effects.retain(|ve| ve.lifetime > 0.0);

        // 戦闘ステップ進行
        let mut collected_events = Vec::new();
        let mut finished_rewards = None;

        if let Some(ref mut session) = self.session {
            if !session.is_finished {
                let ticks_to_run = match self.play_speed {
                    PlaySpeed::Paused => 0,
                    PlaySpeed::Normal => {
                        self.tick_accumulator += dt;
                        if self.tick_accumulator >= 0.08 {
                            self.tick_accumulator -= 0.08;
                            1
                        } else {
                            0
                        }
                    }
                    PlaySpeed::Fast => {
                        self.tick_accumulator += dt;
                        let mut count = 0;
                        while self.tick_accumulator >= 0.025 && count < 4 {
                            self.tick_accumulator -= 0.025;
                            count += 1;
                        }
                        count
                    }
                    PlaySpeed::Instant => 15,
                };

                for _ in 0..ticks_to_run {
                    if session.is_finished {
                        break;
                    }
                    let events = session.step_tick(&self.db);
                    collected_events.extend(events);
                }

                // 決着時の報酬抽出
                if session.is_finished {
                    if session.is_victory && session.is_monster_battle {
                        finished_rewards = Some((
                            session.final_loot_value,
                            session.gained_exp,
                            session.drops.clone(),
                        ));
                    }
                }
            }
        }

        // sessionの借用解放後にイベントと報酬を反映
        if !collected_events.is_empty() {
            self.process_visual_events(collected_events);
        }

        if let Some((final_loot_value, gained_exp, drops)) = finished_rewards {
            self.world.treasury_zeny += final_loot_value;
            for drop in &drops {
                *self.world.storage_items.entry(drop.id.clone()).or_insert(0) += 1;
            }
            if gained_exp > 0 {
                let mut party = self.get_player_party();
                let exp_each = gained_exp / party.len().max(1) as u64;
                let mut exp_logs = Vec::new();
                for member in &mut party {
                    let lvs = Progression::grant_exp(member, exp_each, GrowthPlan::AgiCrit, &mut exp_logs);
                    if lvs > 0 {
                        self.chat_logs.push(format!("🎉 【レベルアップ】{} は Lv.{} に到達！", member.name, member.level));
                    }
                }
                if !party.is_empty() {
                    self.world.active_character = party.remove(0);
                    self.world.party_members = party;
                }
            }
        }
    }

    /// 戦闘イベントをビジュアルエフェクトに変換
    fn process_visual_events(&mut self, events: Vec<BattleVisualEvent>) {
        for ev in events {
            match ev {
                BattleVisualEvent::DamageDealt { attacker_id: _, target_id: _, target_hex, amount, is_crit, skill_name } => {
                    let (tx, ty) = self.hex_to_screen(target_hex);
                    let (text, color, is_big) = if let Some(sk) = skill_name {
                        (format!("{}! -{}", sk, amount), Color::new(1.0, 0.7, 0.2, 1.0), true)
                    } else if is_crit {
                        (format!("CRIT! -{}", amount), Color::new(1.0, 0.85, 0.1, 1.0), true)
                    } else {
                        (format!("-{}", amount), Color::new(0.95, 0.95, 0.95, 1.0), false)
                    };

                    self.floating_texts.push(FloatingText {
                        text,
                        x: tx - 15.0,
                        y: ty - 25.0,
                        color,
                        lifetime: 1.2,
                        max_lifetime: 1.2,
                        is_big,
                    });

                    self.visual_effects.push(VisualEffect {
                        from_x: tx,
                        from_y: ty,
                        to_x: tx,
                        to_y: ty,
                        kind: EffectKind::Slash { color },
                        lifetime: 0.25,
                        max_lifetime: 0.25,
                    });
                }
                BattleVisualEvent::AttackMissed { attacker_id: _, target_id: _, target_hex } => {
                    let (tx, ty) = self.hex_to_screen(target_hex);
                    self.floating_texts.push(FloatingText {
                        text: "MISS".to_string(),
                        x: tx - 12.0,
                        y: ty - 22.0,
                        color: Color::new(0.4, 0.8, 1.0, 1.0),
                        lifetime: 1.0,
                        max_lifetime: 1.0,
                        is_big: false,
                    });
                }
                BattleVisualEvent::AreaEffectTriggered { center, radius, skill_name: _ } => {
                    let (cx, cy) = self.hex_to_screen(center);
                    self.visual_effects.push(VisualEffect {
                        from_x: cx,
                        from_y: cy,
                        to_x: cx,
                        to_y: cy,
                        kind: EffectKind::AoEBlast {
                            radius_hexes: radius,
                            color: Color::new(1.0, 0.35, 0.1, 0.65),
                        },
                        lifetime: 0.45,
                        max_lifetime: 0.45,
                    });
                }
                BattleVisualEvent::UnitKnockedBack { unit_id: _, from, to } => {
                    let (fx, fy) = self.hex_to_screen(from);
                    let (tx, ty) = self.hex_to_screen(to);
                    self.visual_effects.push(VisualEffect {
                        from_x: fx,
                        from_y: fy,
                        to_x: tx,
                        to_y: ty,
                        kind: EffectKind::LaserLine { color: Color::new(0.7, 0.9, 1.0, 0.8) },
                        lifetime: 0.3,
                        max_lifetime: 0.3,
                    });
                }
                BattleVisualEvent::LogMessage(msg) => {
                    self.chat_logs.push(msg);
                    if self.chat_logs.len() > 100 {
                        self.chat_logs.remove(0);
                    }
                }
                _ => {}
            }
        }
    }

    /// 画面全体の描画
    pub fn draw(&mut self) {
        clear_background(Color::new(0.06, 0.08, 0.11, 1.0));

        // 1. 中央・左メインビュー：2.5Dクォータービュー・ヘクス戦場
        self.draw_battlefield();

        // 2. 左下パネル：RO風チャット＆システムログウィンドウ
        self.draw_chat_panel();

        // 3. 右側パネル：ギルド司令部ダッシュボード
        self.draw_dashboard();

        // 4. 精錬ポップアップ
        self.draw_refine_popup();
    }

    /// 2.5Dクォータービュー戦場とユニットの描画
    fn draw_battlefield(&mut self) {
        let mouse_pos = mouse_position();
        let hovered_hex = self.screen_to_hex(mouse_pos.0, mouse_pos.1);

        // マウス左クリックでユニット選択
        if is_mouse_button_pressed(MouseButton::Left) && mouse_pos.0 < 820.0 && mouse_pos.1 < 540.0 {
            self.selected_unit = None;
            if let Some(ref session) = self.session {
                for (idx, c) in session.team_a.iter().enumerate() {
                    if c.is_alive && c.position == hovered_hex {
                        self.selected_unit = Some((true, idx));
                        break;
                    }
                }
                if self.selected_unit.is_none() {
                    for (idx, c) in session.team_b.iter().enumerate() {
                        if c.is_alive && c.position == hovered_hex {
                            self.selected_unit = Some((false, idx));
                            break;
                        }
                    }
                }
            }
        }

        if let Some(ref session) = self.session {
            let map = &session.map;

            // タイトルとTick表示
            self.draw_text_helper(
                &format!("🗺️ 戦場: 『{}』 (Tick: {})", map.name, session.current_tick),
                25.0,
                35.0,
                20.0,
                Color::new(0.9, 0.95, 1.0, 1.0),
            );

            // 1. ヘクスタイル床の描画
            for r in map.min_r..=map.max_r {
                for q in map.min_q..=map.max_q {
                    let coord = HexCoord::new(q, r);
                    let (cx, cy) = self.hex_to_screen(coord);

                    if cx < -50.0 || cx > 870.0 || cy < -50.0 || cy > 560.0 {
                        continue;
                    }

                    let is_blocked = map.blocked_hexes.contains(&coord);
                    let is_hover = coord == hovered_hex;

                    if !is_blocked {
                        // 通常の歩行可能タイル
                        let tile_color = if is_hover {
                            Color::new(0.25, 0.38, 0.45, 0.9)
                        } else {
                            Color::new(0.13, 0.17, 0.21, 0.85)
                        };
                        self.draw_flat_hex(cx, cy, self.hex_size, tile_color, Color::new(0.2, 0.28, 0.35, 0.6));
                    }
                }
            }

            // 2. 障害物・建物の2.5D立体直方体描画
            let wall_height = 20.0;
            for r in map.min_r..=map.max_r {
                for q in map.min_q..=map.max_q {
                    let coord = HexCoord::new(q, r);
                    if map.blocked_hexes.contains(&coord) {
                        let (cx, cy) = self.hex_to_screen(coord);
                        if cx >= -50.0 && cx <= 870.0 && cy >= -50.0 && cy <= 560.0 {
                            self.draw_25d_building_block(cx, cy, self.hex_size, wall_height);
                        }
                    }
                }
            }

            // 3. ホバーヘクスハイライト
            let (hcx, hcy) = self.hex_to_screen(hovered_hex);
            if hcx >= 0.0 && hcx <= 820.0 && hcy >= 0.0 && hcy <= 540.0 {
                self.draw_hex_outline(hcx, hcy, self.hex_size, Color::new(0.3, 0.8, 1.0, 0.8), 2.0);
            }

            // 4. 選択中ユニットの射程リング描画
            if let Some((is_team_a, idx)) = self.selected_unit {
                let unit_opt = if is_team_a {
                    session.team_a.get(idx)
                } else {
                    session.team_b.get(idx)
                };
                if let Some(u) = unit_opt {
                    if u.is_alive {
                        let (ux, uy) = self.hex_to_screen(u.position);
                        let ring_rad = u.attack_range as f32 * self.hex_size * 1.73;
                        draw_circle_lines(ux, uy, ring_rad, 1.5, Color::new(1.0, 0.8, 0.2, 0.6));
                    }
                }
            }

            // 5. ユニットの描画（Yソートして手前から順に描画）
            let mut all_units: Vec<(bool, usize, &Combatant)> = Vec::new();
            for (idx, c) in session.team_a.iter().enumerate() {
                if c.is_alive {
                    all_units.push((true, idx, c));
                }
            }
            for (idx, c) in session.team_b.iter().enumerate() {
                if c.is_alive {
                    all_units.push((false, idx, c));
                }
            }
            all_units.sort_by(|a, b| {
                let (_, ya) = self.hex_to_screen(a.2.position);
                let (_, yb) = self.hex_to_screen(b.2.position);
                ya.partial_cmp(&yb).unwrap_or(std::cmp::Ordering::Equal)
            });

            for (is_team_a, idx, unit) in all_units {
                self.draw_unit(unit, is_team_a, idx, session.is_monster_battle);
            }

            // 6. 視覚エフェクトの描画（レーザー、爆炎、斬撃）
            for ve in &self.visual_effects {
                let alpha = (ve.lifetime / ve.max_lifetime).clamp(0.0, 1.0);
                match ve.kind {
                    EffectKind::LaserLine { color } => {
                        let c = Color::new(color.r, color.g, color.b, color.a * alpha);
                        draw_line(ve.from_x, ve.from_y, ve.to_x, ve.to_y, 3.0, c);
                    }
                    EffectKind::AoEBlast { radius_hexes, color } => {
                        let rad = (radius_hexes as f32 + 0.8) * self.hex_size * 1.5;
                        let c = Color::new(color.r, color.g, color.b, color.a * alpha);
                        draw_circle(ve.to_x, ve.to_y, rad, c);
                        draw_circle_lines(ve.to_x, ve.to_y, rad, 2.0, Color::new(1.0, 0.9, 0.5, alpha));
                    }
                    EffectKind::Slash { color } => {
                        let c = Color::new(color.r, color.g, color.b, color.a * alpha);
                        draw_line(ve.to_x - 12.0, ve.to_y - 12.0, ve.to_x + 12.0, ve.to_y + 12.0, 2.5, c);
                        draw_line(ve.to_x + 12.0, ve.to_y - 12.0, ve.to_x - 12.0, ve.to_y + 12.0, 2.5, c);
                    }
                }
            }

            // 7. フローティングテキストの描画
            for ft in &self.floating_texts {
                let alpha = (ft.lifetime / ft.max_lifetime).clamp(0.0, 1.0);
                let c = Color::new(ft.color.r, ft.color.g, ft.color.b, alpha);
                let size = if ft.is_big { 18.0 } else { 14.0 };
                self.draw_text_helper(&ft.text, ft.x, ft.y, size, c);
            }
        }
    }

    /// 六角形タイルの描画（クォータービュー圧縮: y比率0.58）
    fn draw_flat_hex(&self, cx: f32, cy: f32, size: f32, fill: Color, outline: Color) {
        let mut points = [Vec2::ZERO; 6];
        for k in 0..6 {
            let angle = (60.0 * k as f32 - 30.0) * PI / 180.0;
            points[k] = Vec2::new(cx + size * angle.cos(), cy + size * angle.sin() * 0.58);
        }

        // 三角形ファンで塗りつぶし
        for k in 1..5 {
            draw_triangle(points[0], points[k], points[k + 1], fill);
        }
        for k in 0..6 {
            let next = (k + 1) % 6;
            draw_line(points[k].x, points[k].y, points[next].x, points[next].y, 1.0, outline);
        }
    }

    /// 六角形の輪郭線描画
    fn draw_hex_outline(&self, cx: f32, cy: f32, size: f32, outline: Color, thickness: f32) {
        let mut points = [Vec2::ZERO; 6];
        for k in 0..6 {
            let angle = (60.0 * k as f32 - 30.0) * PI / 180.0;
            points[k] = Vec2::new(cx + size * angle.cos(), cy + size * angle.sin() * 0.58);
        }
        for k in 0..6 {
            let next = (k + 1) % 6;
            draw_line(points[k].x, points[k].y, points[next].x, points[next].y, thickness, outline);
        }
    }

    /// 建物・防壁の2.5D立体ブロック描画
    fn draw_25d_building_block(&self, cx: f32, cy: f32, size: f32, height: f32) {
        let mut bottom = [Vec2::ZERO; 6];
        let mut top = [Vec2::ZERO; 6];

        for k in 0..6 {
            let angle = (60.0 * k as f32 - 30.0) * PI / 180.0;
            bottom[k] = Vec2::new(cx + size * angle.cos(), cy + size * angle.sin() * 0.58);
            top[k] = Vec2::new(cx + size * angle.cos(), cy - height + size * angle.sin() * 0.58);
        }

        // 前面3辺の側面ポリゴン（暗い石壁の陰影）
        let side_color_dark = Color::new(0.22, 0.25, 0.30, 1.0);
        let side_color_light = Color::new(0.30, 0.34, 0.40, 1.0);

        // 南東面 (k=1 to 2)
        draw_triangle(bottom[1], bottom[2], top[2], side_color_light);
        draw_triangle(bottom[1], top[2], top[1], side_color_light);

        // 南面 (k=2 to 3)
        draw_triangle(bottom[2], bottom[3], top[3], side_color_dark);
        draw_triangle(bottom[2], top[3], top[2], side_color_dark);

        // 南西面 (k=3 to 4)
        draw_triangle(bottom[3], bottom[4], top[4], side_color_dark);
        draw_triangle(bottom[3], top[4], top[3], side_color_dark);

        // 上面（明るい天板）
        let top_color = Color::new(0.42, 0.48, 0.56, 1.0);
        for k in 1..5 {
            draw_triangle(top[0], top[k], top[k + 1], top_color);
        }

        // 上面枠線
        for k in 0..6 {
            let next = (k + 1) % 6;
            draw_line(top[k].x, top[k].y, top[next].x, top[next].y, 1.2, Color::new(0.55, 0.62, 0.72, 1.0));
        }
        // 側面の縦エッジライン
        draw_line(bottom[2].x, bottom[2].y, top[2].x, top[2].y, 1.2, Color::new(0.18, 0.20, 0.25, 1.0));
        draw_line(bottom[3].x, bottom[3].y, top[3].x, top[3].y, 1.2, Color::new(0.18, 0.20, 0.25, 1.0));
    }

    /// ユニット（キャラクター / モンスター）の描画
    fn draw_unit(&self, unit: &Combatant, is_team_a: bool, idx: usize, is_monster_battle: bool) {
        let (cx, cy) = self.hex_to_screen(unit.position);

        // 扁平な影
        draw_circle(cx, cy + 2.0, self.hex_size * 0.55, Color::new(0.0, 0.0, 0.0, 0.45));

        // トークン色設定
        let is_selected = self.selected_unit == Some((is_team_a, idx));
        let is_leech_target = is_team_a && unit.character.level < 15; // 養殖対象Lv

        let (body_color, border_color) = if is_team_a {
            if is_leech_target {
                (Color::new(0.95, 0.8, 0.2, 1.0), Color::new(1.0, 1.0, 0.6, 1.0)) // 養殖キャラ（金色）
            } else {
                (Color::new(0.2, 0.55, 0.9, 1.0), Color::new(0.6, 0.8, 1.0, 1.0)) // 味方（青色）
            }
        } else {
            if is_monster_battle {
                (Color::new(0.85, 0.25, 0.3, 1.0), Color::new(1.0, 0.5, 0.5, 1.0)) // モンスター（赤色）
            } else {
                (Color::new(0.8, 0.4, 0.1, 1.0), Color::new(1.0, 0.65, 0.3, 1.0)) // GvGライバル（橙色）
            }
        };

        // トークン本体円盤
        let unit_radius = 12.0;
        let unit_y = cy - 8.0;
        draw_circle(cx, unit_y, unit_radius, body_color);
        draw_circle_lines(cx, unit_y, unit_radius, if is_selected { 2.5 } else { 1.5 }, if is_selected { YELLOW } else { border_color });

        // 養殖対象キャラには★マーク
        if is_leech_target {
            self.draw_text_helper("★", cx - 4.0, unit_y + 4.0, 12.0, BLACK);
        } else {
            // ロール記号または1文字目
            let symbol = unit.character.name.chars().next().unwrap_or('?').to_string();
            self.draw_text_helper(&symbol, cx - 4.0, unit_y + 4.0, 11.0, WHITE);
        }

        // 頭上HPバー
        let bar_w = 34.0;
        let bar_h = 4.0;
        let bar_x = cx - bar_w / 2.0;
        let bar_y = unit_y - 18.0;

        let hp_ratio = (unit.stats.current_hp.max(0) as f32 / unit.stats.max_hp.max(1) as f32).clamp(0.0, 1.0);
        draw_rectangle(bar_x, bar_y, bar_w, bar_h, Color::new(0.2, 0.2, 0.2, 0.85));
        let hp_color = if hp_ratio > 0.5 {
            Color::new(0.2, 0.85, 0.3, 1.0)
        } else if hp_ratio > 0.2 {
            Color::new(0.9, 0.8, 0.1, 1.0)
        } else {
            Color::new(0.9, 0.2, 0.2, 1.0)
        };
        draw_rectangle(bar_x, bar_y, bar_w * hp_ratio, bar_h, hp_color);
        draw_rectangle_lines(bar_x, bar_y, bar_w, bar_h, 1.0, Color::new(0.1, 0.1, 0.1, 0.9));

        // 名前とLvの簡易表示
        let label = format!("Lv.{} {}", unit.character.level, unit.character.name);
        self.draw_text_helper(&label, bar_x - 10.0, bar_y - 4.0, 10.0, Color::new(0.9, 0.95, 1.0, 0.9));
    }

    /// 左下パネル：RO風チャット＆システムログウィンドウ
    fn draw_chat_panel(&self) {
        let x = 20.0;
        let y = 550.0;
        let w = 800.0;
        let h = 230.0;

        draw_rectangle(x, y, w, h, Color::new(0.05, 0.07, 0.10, 0.90));
        draw_rectangle_lines(x, y, w, h, 1.5, Color::new(0.25, 0.35, 0.48, 0.8));

        self.draw_text_helper("📜 [ギルド通信＆戦闘ログ]", x + 15.0, y + 22.0, 14.0, Color::new(0.5, 0.8, 1.0, 1.0));

        let start_idx = if self.chat_logs.len() > 9 {
            self.chat_logs.len() - 9
        } else {
            0
        };

        for (i, log) in self.chat_logs[start_idx..].iter().enumerate() {
            let line_y = y + 46.0 + (i as f32 * 19.0);
            let log_color = if log.contains("クホホ") || log.contains("倒れた") || log.contains("全滅") {
                Color::new(1.0, 0.45, 0.55, 1.0) // 警告・ロスト・死亡
            } else if log.contains("レベルアップ") || log.contains("成功") || log.contains("撃退") {
                Color::new(0.3, 0.95, 0.6, 1.0) // 歓喜・成功
            } else if log.contains("Tick") {
                Color::new(0.85, 0.88, 0.92, 0.9) // 戦闘ステップ
            } else {
                Color::new(0.7, 0.85, 0.95, 0.95) // システムメッセージ
            };

            self.draw_text_helper(log, x + 15.0, line_y, 13.0, log_color);
        }
    }

    /// 右側パネル：ギルド司令部ダッシュボード
    fn draw_dashboard(&mut self) {
        let x = 840.0;
        let y = 20.0;
        let w = 420.0;
        let h = 760.0;

        draw_rectangle(x, y, w, h, Color::new(0.08, 0.11, 0.16, 0.96));
        draw_rectangle_lines(x, y, w, h, 2.0, Color::new(0.35, 0.48, 0.65, 0.9));

        // ギルド概要ヘッダー
        let total_party_count = 1 + self.world.party_members.len();
        self.draw_text_helper("🏰 ギルド司令部: 『Realm of Origins』", x + 20.0, y + 30.0, 17.0, Color::new(1.0, 0.85, 0.3, 1.0));
        self.draw_text_helper(&format!("世代: 第{}世代 | 資金: {} Zeny", self.world.generation, self.world.treasury_zeny), x + 20.0, y + 54.0, 14.0, Color::new(0.8, 0.9, 1.0, 0.95));
        self.draw_text_helper(
            &format!("主力: {}名 | 控え名簿: {}名 | 倉庫品: {}種", total_party_count, self.world.roster.active_members.len(), self.world.storage_items.len()),
            x + 20.0,
            y + 74.0,
            13.0,
            Color::new(0.7, 0.8, 0.9, 0.9),
        );

        draw_line(x + 15.0, y + 88.0, x + w - 15.0, y + 88.0, 1.0, Color::new(0.3, 0.4, 0.5, 0.6));

        // 1. 再生速度コントロール
        self.draw_text_helper("⏱️ 戦闘速度:", x + 20.0, y + 112.0, 14.0, WHITE);
        let btn_y = y + 96.0;
        if self.draw_button(Rect::new(x + 110.0, btn_y, 50.0, 24.0), "一時停止", self.play_speed == PlaySpeed::Paused, Color::new(0.2, 0.25, 0.32, 1.0)) {
            self.play_speed = PlaySpeed::Paused;
        }
        if self.draw_button(Rect::new(x + 170.0, btn_y, 50.0, 24.0), "通常1x", self.play_speed == PlaySpeed::Normal, Color::new(0.2, 0.25, 0.32, 1.0)) {
            self.play_speed = PlaySpeed::Normal;
        }
        if self.draw_button(Rect::new(x + 230.0, btn_y, 50.0, 24.0), "高速3x", self.play_speed == PlaySpeed::Fast, Color::new(0.2, 0.25, 0.32, 1.0)) {
            self.play_speed = PlaySpeed::Fast;
        }
        if self.draw_button(Rect::new(x + 290.0, btn_y, 65.0, 24.0), "最大速", self.play_speed == PlaySpeed::Instant, Color::new(0.2, 0.25, 0.32, 1.0)) {
            self.play_speed = PlaySpeed::Instant;
        }

        // 2. バトル開始アクション
        let mut act_y = y + 135.0;
        self.draw_text_helper("⚔️ 作戦展開 ＆ 戦闘開始:", x + 20.0, act_y + 16.0, 14.0, Color::new(0.9, 0.7, 0.3, 1.0));
        act_y += 24.0;

        // 【隘路防衛 ＆ 養殖バトル】ボタン
        if self.draw_button(Rect::new(x + 20.0, act_y, 380.0, 32.0), "🛡️ 監視塔の隘路防衛 ＆ 養殖バトル (空間防衛)", false, Color::new(0.18, 0.30, 0.45, 1.0)) {
            let engine = SimulationEngine::new(&self.db);
            let map = HexMap::create_chokepoint_map();
            let party = self.get_player_party();
            let template_high_orc = self.db.monsters.get("high_orc").expect("high_orc exists");
            let template_orc = self.db.monsters.get("orc_warrior").expect("orc_warrior exists");
            let m1 = engine.spawn_monster(template_high_orc, MonsterArchetype::Heavy);
            let m2 = engine.spawn_monster(template_orc, MonsterArchetype::Swift);
            let monsters = vec![m1, m2];
            self.session = Some(engine.create_chokepoint_session(map, &party, &monsters));
            self.chat_logs.push("🛡️ 『監視塔の隘路』防衛バトルを開始！ 前衛がチョークポイントを封鎖します。".to_string());
        }
        act_y += 38.0;

        // 【空間GvG模擬戦】ボタン
        if self.draw_button(Rect::new(x + 20.0, act_y, 380.0, 32.0), "🏆 空間GvG模擬戦：宿敵『黒曜の旅団』激突", false, Color::new(0.35, 0.20, 0.35, 1.0)) {
            let engine = SimulationEngine::new(&self.db);
            let map = HexMap::create_gvg_arena_map();
            let party = self.get_player_party();
            let rival_party = if let Some(rival) = self.world.rival_guilds.iter().find(|g| g.name.contains("黒曜")) {
                rival.roster.clone()
            } else {
                party.clone()
            };
            self.session = Some(engine.create_gvg_session(
                map,
                &party,
                &rival_party,
                "自軍ギルド",
                "黒曜の旅団",
                None,
            ));
            self.chat_logs.push("🏆 空間GvG模擬戦が開幕！ 栄光の闘技場で陣形が激突します。".to_string());
        }
        act_y += 45.0;

        draw_line(x + 15.0, act_y, x + w - 15.0, act_y, 1.0, Color::new(0.3, 0.4, 0.5, 0.6));
        act_y += 10.0;

        // 3. 経済＆ギルド運営ボタン群
        self.draw_text_helper("⚒️ ギルド運営 ＆ 経済操作:", x + 20.0, act_y + 16.0, 14.0, Color::new(0.4, 0.85, 0.9, 1.0));
        act_y += 24.0;

        // 【クホホ精錬オーダー】
        if self.draw_button(Rect::new(x + 20.0, act_y, 185.0, 30.0), "🔨 クホホ精錬オーダー", false, Color::new(0.35, 0.25, 0.15, 1.0)) {
            self.execute_refine_action();
        }

        // 【市場ハック＆自動派遣】
        if self.draw_button(Rect::new(x + 215.0, act_y, 185.0, 30.0), "📈 市場連動 ＆ 自動派遣", false, Color::new(0.20, 0.35, 0.25, 1.0)) {
            self.execute_market_and_dispatch_action();
        }
        act_y += 36.0;

        // 【レガシー世代交代】
        if self.draw_button(Rect::new(x + 20.0, act_y, 185.0, 30.0), "🔄 レガシー世代交代", false, Color::new(0.30, 0.20, 0.40, 1.0)) {
            self.execute_succession_action();
        }

        // 【セーブ ＆ ロード】
        let is_save = self.draw_button(Rect::new(x + 215.0, act_y, 90.0, 30.0), "💾 セーブ", false, Color::new(0.18, 0.25, 0.35, 1.0));
        let is_load = self.draw_button(Rect::new(x + 310.0, act_y, 90.0, 30.0), "📂 ロード", false, Color::new(0.18, 0.25, 0.35, 1.0));

        if is_save {
            match self.world.save_to_file("saves/save_data.ron") {
                Ok(_) => {
                    self.chat_logs.push("💾 [セーブ完了] 'saves/save_data.ron' へ状態を保存しました。".to_string());
                }
                Err(e) => {
                    self.chat_logs.push(format!("❌ [セーブ失敗] {}", e));
                }
            }
        }
        if is_load {
            match WorldState::load_from_file("saves/save_data.ron") {
                Ok(loaded) => {
                    self.world = loaded;
                    self.chat_logs.push("📂 [ロード完了] 'saves/save_data.ron' からギルド状態を復元しました。".to_string());
                }
                Err(e) => {
                    self.chat_logs.push(format!("❌ [ロード失敗] {}", e));
                }
            }
        }
        act_y += 45.0;

        draw_line(x + 15.0, act_y, x + w - 15.0, act_y, 1.0, Color::new(0.3, 0.4, 0.5, 0.6));
        act_y += 10.0;

        // 4. 選択ユニットの詳細ステータスビュー
        self.draw_selected_unit_details(x + 20.0, act_y);
    }

    /// 選択ユニットの詳細情報描画
    fn draw_selected_unit_details(&self, start_x: f32, start_y: f32) {
        self.draw_text_helper("👤 ユニット詳細ステータス:", start_x, start_y + 14.0, 14.0, Color::new(0.9, 0.9, 0.5, 1.0));

        let unit_opt = if let Some(ref session) = self.session {
            if let Some((is_team_a, idx)) = self.selected_unit {
                if is_team_a {
                    session.team_a.get(idx)
                } else {
                    session.team_b.get(idx)
                }
            } else {
                session.team_a.first()
            }
        } else {
            None
        };

        if let Some(u) = unit_opt {
            let c = &u.character;
            let s = &u.stats;
            let mut y = start_y + 36.0;

            self.draw_text_helper(&format!("名前: {}  (職業: {})", c.name, c.job_id), start_x, y, 14.0, WHITE);
            y += 20.0;
            self.draw_text_helper(
                &format!("Lv: {}  |  HP: {}/{}  |  射程: {}", c.level, s.current_hp, s.max_hp, u.attack_range),
                start_x,
                y,
                13.0,
                Color::new(0.8, 0.95, 1.0, 0.9),
            );
            y += 20.0;
            self.draw_text_helper(
                &format!("ATK: {}〜{} | MATK: {} | DEF: {}+{}", s.min_atk, s.max_atk, s.matk, s.equip_def, s.vit_def),
                start_x,
                y,
                12.0,
                Color::new(0.85, 0.85, 0.9, 0.85),
            );
            y += 18.0;
            self.draw_text_helper(
                &format!("HIT: {} | FLEE: {} | CRIT: {:.1}% | ASPD: {:.1}", s.hit, s.flee, s.crit, s.aspd),
                start_x,
                y,
                12.0,
                Color::new(0.85, 0.85, 0.9, 0.85),
            );
            y += 22.0;

            // 装備品
            let weapon_name = c.equipments.iter().find(|e| e.slot == crate::models::EquipSlot::Weapon)
                .map(|w| format!("+{} {}", w.refine, w.name))
                .unwrap_or_else(|| "素手".to_string());
            self.draw_text_helper(&format!("主武器: {}", weapon_name), start_x, y, 13.0, Color::new(1.0, 0.8, 0.4, 0.95));
            y += 18.0;

            // 陣形とガンビット
            let role_name = match c.formation.role {
                FormationRole::Frontline => "前衛 (Frontline)",
                FormationRole::Midline => "中衛 (Midline)",
                FormationRole::Backline => "後衛 (Backline)",
            };
            self.draw_text_helper(&format!("配置ロール: {}", role_name), start_x, y, 12.0, Color::new(0.7, 0.9, 1.0, 0.9));
            y += 18.0;

            let gambit_summary = if c.gambit_rules.is_empty() {
                "通常攻撃のみ".to_string()
            } else {
                format!("ルール数: {}件 (最優先スキル使用)", c.gambit_rules.len())
            };
            self.draw_text_helper(&format!("戦術作戦: {}", gambit_summary), start_x, y, 12.0, Color::new(0.7, 0.9, 0.7, 0.9));
        } else {
            self.draw_text_helper("戦場上のユニットをクリックすると詳細が表示されます。", start_x, start_y + 40.0, 12.0, GRAY);
        }
    }

    /// クホホ精錬オーダーの実行
    fn execute_refine_action(&mut self) {
        let cost = 3000u32;
        if self.world.treasury_zeny < cost {
            self.chat_logs.push("❌ 精錬資金が足りません (必要: 3,000 Zeny)".to_string());
            return;
        }
        self.world.treasury_zeny -= cost;

        // 主力の最初の武器を精錬
        let mut target_found = false;
        let mut refine_result = false;
        let mut msg = String::new();

        if let Some(eq) = self.world.active_character.equipments.iter_mut().find(|e| e.slot == crate::models::EquipSlot::Weapon) {
            target_found = true;
            let old_refine = eq.refine;
            let mut refine_logs = Vec::new();
            match Refinery::refine_once(eq, None, &mut refine_logs) {
                Ok(true) => {
                    refine_result = true;
                    msg = format!("✨ 【カンカンカン…大成功！】{} の {} が +{} に強化されました！", self.world.active_character.name, eq.name, eq.refine);
                }
                Ok(false) => {
                    refine_result = false;
                    msg = format!("💥 【クホホホホ…！】過剰精錬失敗！ +{} {} は粉々に砕け散りました…！", old_refine, eq.name);
                }
                Err(err) => {
                    refine_result = false;
                    msg = format!("💨 精錬中断: {}", err);
                }
            }
        }

        if !target_found {
            msg = "武器を装備しているキャラクターがいません。".to_string();
        }

        self.chat_logs.push(msg.clone());
        self.refine_popup = Some((msg, refine_result, 3.0));
    }

    /// 市場ハック ＆ 自動派遣の実行
    fn execute_market_and_dispatch_action(&mut self) {
        // 1. 市場で完成品武器の買い占めシミュレーション
        let item_id = "steel";
        let buy_count = 5;

        match self.world.market.buy(item_id, buy_count) {
            Ok(total_cost) => {
                if self.world.treasury_zeny >= total_cost {
                    self.world.treasury_zeny -= total_cost;
                    *self.world.storage_items.entry(item_id.to_string()).or_insert(0) += buy_count;
                    self.chat_logs.push(format!(
                        "📈 [市場ハック] {} を{}個買い占め！ (合計: {} Zeny, BOM需要連動中)",
                        item_id, buy_count, total_cost
                    ));
                } else {
                    self.chat_logs.push("❌ 資金不足で市場買い占めに失敗しました。".to_string());
                }
            }
            Err(e) => {
                self.chat_logs.push(format!("❌ [市場エラー] {}", e));
            }
        }

        // 2. 控え部隊の自動派遣
        if !self.world.roster.active_members.is_empty() {
            let scout = self.world.roster.active_members.remove(0);
            let reward_zeny = (scout.level as u32 * 800) + 2000;
            self.world.treasury_zeny += reward_zeny;
            *self.world.storage_items.entry("iron_ore".to_string()).or_insert(0) += 3;
            self.chat_logs.push(format!(
                "⛏️ [自動派遣帰還] 控えの {} が『オーク地下洞窟』から帰還！ (+{} Zeny, 鉄鉱石x3獲得)",
                scout.name, reward_zeny
            ));
            self.world.roster.active_members.push(scout);
        }
    }

    /// 世代交代（レガシー転生）の実行
    fn execute_succession_action(&mut self) {
        let old_hero = self.world.active_character.clone();
        self.world.generation += 1;

        // 旧主人公を控え名簿へ移籍
        self.world.roster.active_members.push(old_hero.clone());

        // 次世代の新キャラクター加入
        let new_char = Progression::create_reincarnated_character(
            &format!("gen{}_hero", self.world.generation),
            &format!("第{}世代ルーキー", self.world.generation),
            "swordsman",
            1,
            GrowthPlan::VitTank,
        );
        self.world.active_character = new_char.clone();

        self.chat_logs.push(format!(
            "🔄 【レガシー世代交代】先代英雄 {} が名簿へ！ 第{}世代へ継承され、{} (Lv.1) が加入しました！",
            old_hero.name, self.world.generation, new_char.name
        ));
    }

    /// 精錬成功/クホホ失敗ポップアップ描画
    fn draw_refine_popup(&self) {
        if let Some((ref msg, is_success, _)) = self.refine_popup {
            let cx = 450.0;
            let cy = 250.0;
            let pw = 480.0;
            let ph = 80.0;
            let px = cx - pw / 2.0;
            let py = cy - ph / 2.0;

            let (bg, border) = if is_success {
                (Color::new(0.08, 0.25, 0.15, 0.95), Color::new(0.4, 1.0, 0.6, 1.0))
            } else {
                (Color::new(0.35, 0.08, 0.10, 0.95), Color::new(1.0, 0.3, 0.4, 1.0))
            };

            draw_rectangle(px, py, pw, ph, bg);
            draw_rectangle_lines(px, py, pw, ph, 2.5, border);
            self.draw_text_helper(msg, px + 20.0, py + 46.0, 14.0, WHITE);
        }
    }
}

/// Macroquad GUIメインエントリ
pub async fn run_gui(db: ModDatabase) {
    let mut app = GuiApp::new(db);

    loop {
        let dt = get_frame_time();
        app.update(dt);
        app.draw();
        next_frame().await;
    }
}
