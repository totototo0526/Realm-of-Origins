use std::f32::consts::PI;
use macroquad::prelude::*;

use crate::engine::{BattleVisualEvent, Combatant, SimulationEngine, SpatialBattleSession};
use crate::hex_grid::{HexCoord, HexMap};
use crate::loader::ModDatabase;
use crate::models::{
    Action, Card, Character, Condition, EquipSlot, Equipment, FormationRole,
    MonsterArchetype,
};
use crate::progression::{GrowthPlan, Progression};
use crate::refinery::{Refinery, WorkOrder, WorkOrderMode};
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

/// 攻撃光線や範囲爆発、画面フラッシュなどの視覚エフェクト
#[derive(Debug, Clone)]
enum EffectKind {
    LaserLine { color: Color },
    AoEBlast { radius_hexes: u32, color: Color },
    Slash { color: Color },
    ScreenFlash { color: Color }, // 精錬成功/失敗の画面フラッシュ演出
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

/// 弓の矢や魔法の火球などの弾道パーティクル
#[derive(Debug, Clone)]
struct TrajectoryParticle {
    from_x: f32,
    from_y: f32,
    to_x: f32,
    to_y: f32,
    progress: f32, // 0.0 .. 1.0
    speed: f32,    // 1秒あたりの進行率
    color: Color,
    is_arrow: bool,
    target_hex: HexCoord,
    damage_text: Option<(String, Color, bool)>,
    is_aoe: bool,
    aoe_radius: u32,
}

/// 再生速度コントロール
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaySpeed {
    Paused,
    Normal,  // 1x (約0.08s / Tick)
    Fast,    // 3x (約0.025s / Tick)
    Instant, // 毎フレーム最大20 Ticks進行
}

/// 右パネルのマルチタブ種別
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DashboardTab {
    Battlefield,    // 1: 戦場/編成
    Workshop,       // 2: 工房/精錬 (クホホ工場)
    MarketDispatch, // 3: 市場/派遣 (BOM経済＆遠征)
    RosterLegacy,   // 4: 名簿/転生 (サクセス×ペナント)
}

/// カメラ状態（パン・ズーム制御）
#[derive(Debug, Clone)]
pub struct CameraState {
    pub pan_x: f32,
    pub pan_y: f32,
    pub zoom: f32,
    pub is_dragging: bool,
    pub drag_start_mouse: (f32, f32),
    pub drag_start_pan: (f32, f32),
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            pan_x: 0.0,
            pan_y: 0.0,
            zoom: 1.0,
            is_dragging: false,
            drag_start_mouse: (0.0, 0.0),
            drag_start_pan: (0.0, 0.0),
        }
    }
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
    trajectory_particles: Vec<TrajectoryParticle>,
    pub camera: CameraState,
    pub active_tab: DashboardTab,
    pub selected_unit: Option<(bool, usize)>, // (is_team_a, index)
    pub refine_popup: Option<(String, bool, f32)>, // (msg, is_success, timer)
    pub selected_storage_equip_idx: Option<usize>,
    pub active_dispatch_progress: Option<(String, String, f32, f32)>, // (area_id, area_name, current_time, total_time)
    pub next_growth_plan: GrowthPlan,
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
        chat_logs.push("[LOG] 『Realm of Origins』ギルド司令部システム起動。".to_string());
        chat_logs.push("[LOG] 右パネル上部タブで『戦場/編成』『工房/精錬』『市場/派遣』『名簿/転生』を切り替え可能。".to_string());
        chat_logs.push(format!("[LOG] ギルド資金: {} Zeny | 世代: 第{}世代", world.treasury_zeny, world.generation));

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
            trajectory_particles: Vec::new(),
            camera: CameraState::default(),
            active_tab: DashboardTab::Battlefield,
            selected_unit: None,
            refine_popup: None,
            selected_storage_equip_idx: None,
            active_dispatch_progress: None,
            next_growth_plan: GrowthPlan::VitTank,
            // 戦場ビューポート（0..830, 0..535）の中央にマップが収まる初期座標
            origin_x: 270.0,
            origin_y: 190.0,
            hex_size: 28.0,
        }
    }

    /// 現在の出撃パーティーを取得
    pub fn get_player_party(&self) -> Vec<Character> {
        let mut party = vec![self.world.active_character.clone()];
        party.extend(self.world.party_members.clone());
        party
    }

    /// ヘクス座標から2.5Dクォータービュー画面ピクセル座標へ変換（カメラのパン＆ズームを考慮）
    pub fn hex_to_screen(&self, coord: HexCoord) -> (f32, f32) {
        let eff_size = self.hex_size * self.camera.zoom;
        let (rx, ry) = HexCoord::hex_to_pixel(coord, eff_size);
        (
            self.origin_x + self.camera.pan_x + rx,
            self.origin_y + self.camera.pan_y + ry,
        )
    }

    /// 画面ピクセル座標からヘクス座標へ逆変換（カメラのパン＆ズームを考慮）
    pub fn screen_to_hex(&self, sx: f32, sy: f32) -> HexCoord {
        let eff_size = self.hex_size * self.camera.zoom;
        let dx = sx - self.origin_x - self.camera.pan_x;
        let dy = sy - self.origin_y - self.camera.pan_y;
        HexCoord::pixel_to_hex(dx, dy, eff_size)
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

    /// カスタムボタン描画（ホバー判定＆クリック検知、自動パディング調整）
    fn draw_button(&self, rect: Rect, label: &str, is_active: bool, base_color: Color) -> bool {
        let mouse_pos = mouse_position();
        let is_hover = rect.contains(Vec2::new(mouse_pos.0, mouse_pos.1));
        let clicked = is_hover && is_mouse_button_pressed(MouseButton::Left);

        let bg_color = if is_active {
            Color::new(0.20, 0.55, 0.85, 0.95)
        } else if is_hover {
            Color::new(
                (base_color.r * 1.3).min(1.0),
                (base_color.g * 1.3).min(1.0),
                (base_color.b * 1.3).min(1.0),
                0.95,
            )
        } else {
            base_color
        };

        draw_rectangle(rect.x, rect.y, rect.w, rect.h, bg_color);
        draw_rectangle_lines(
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            1.2,
            if is_active {
                Color::new(0.8, 0.95, 1.0, 1.0)
            } else {
                Color::new(0.5, 0.6, 0.75, 0.7)
            },
        );

        // 文字がはみ出さないよう高さを基準に適切なフォントサイズを決定
        let font_size = (rect.h * 0.46).clamp(11.0, 14.0);
        let text_x = rect.x + 8.0;
        let text_y = rect.y + rect.h * 0.68;
        let text_color = if is_active {
            WHITE
        } else {
            Color::new(0.92, 0.94, 0.97, 1.0)
        };
        self.draw_text_helper(label, text_x, text_y, font_size, text_color);

        clicked
    }

    /// カメラの入力処理（右ドラッグ/WASDパン、ホイール/QEズーム）
    fn handle_camera_input(&mut self, dt: f32) {
        let mouse_pos = mouse_position();
        let in_viewport = mouse_pos.0 < 830.0 && mouse_pos.1 < 535.0;

        // マウス右ボタンドラッグでパン
        if is_mouse_button_pressed(MouseButton::Right) && in_viewport {
            self.camera.is_dragging = true;
            self.camera.drag_start_mouse = mouse_pos;
            self.camera.drag_start_pan = (self.camera.pan_x, self.camera.pan_y);
        }
        if is_mouse_button_down(MouseButton::Right) && self.camera.is_dragging {
            let dx = mouse_pos.0 - self.camera.drag_start_mouse.0;
            let dy = mouse_pos.1 - self.camera.drag_start_mouse.1;
            self.camera.pan_x = self.camera.drag_start_pan.0 + dx;
            self.camera.pan_y = self.camera.drag_start_pan.1 + dy;
        }
        if is_mouse_button_released(MouseButton::Right) {
            self.camera.is_dragging = false;
        }

        // キーボードWASD/矢印キーでパン
        let pan_speed = 360.0 * dt;
        if is_key_down(KeyCode::W) || is_key_down(KeyCode::Up) {
            self.camera.pan_y += pan_speed;
        }
        if is_key_down(KeyCode::S) || is_key_down(KeyCode::Down) {
            self.camera.pan_y -= pan_speed;
        }
        if is_key_down(KeyCode::A) || is_key_down(KeyCode::Left) {
            self.camera.pan_x += pan_speed;
        }
        if is_key_down(KeyCode::D) || is_key_down(KeyCode::Right) {
            self.camera.pan_x -= pan_speed;
        }

        // マウスホイールでズーム (0.6x 〜 1.8x)
        if in_viewport {
            let (_, wheel_y) = mouse_wheel();
            if wheel_y != 0.0 {
                let zoom_delta = wheel_y * 0.06;
                self.camera.zoom = (self.camera.zoom + zoom_delta).clamp(0.6, 1.8);
            }
        }

        // キーボードQ/Eでズーム
        if is_key_down(KeyCode::Q) {
            self.camera.zoom = (self.camera.zoom - 0.6 * dt).clamp(0.6, 1.8);
        }
        if is_key_down(KeyCode::E) {
            self.camera.zoom = (self.camera.zoom + 0.6 * dt).clamp(0.6, 1.8);
        }

        // Rキーでカメラリセット
        if is_key_pressed(KeyCode::R) {
            self.reset_camera();
        }
    }

    /// カメラ位置とズームを初期化
    pub fn reset_camera(&mut self) {
        self.camera.pan_x = 0.0;
        self.camera.pan_y = 0.0;
        self.camera.zoom = 1.0;
        self.camera.is_dragging = false;
    }

    /// 1フレームごとの更新（戦闘ステップ進行、弾道パーティクル、遠征進捗）
    pub fn update(&mut self, dt: f32) {
        self.handle_camera_input(dt);

        // 精錬ポップアップのタイマー
        if let Some((_, _, ref mut timer)) = self.refine_popup {
            *timer -= dt;
            if *timer <= 0.0 {
                self.refine_popup = None;
            }
        }

        // 遠征（自動派遣）のリアルタイム進捗更新
        if let Some((area_id, area_name, ref mut cur, total)) = self.active_dispatch_progress.clone() {
            let new_cur = *cur + dt;
            if new_cur >= total {
                self.active_dispatch_progress = None;
                // 遠征完了・報酬獲得
                let reward_zeny = 3500;
                self.world.treasury_zeny += reward_zeny;
                let drop_item = if area_id.contains("mine") { "iron_ore" } else { "white_herb" };
                *self.world.storage_items.entry(drop_item.to_string()).or_insert(0) += 8;
                *self.world.storage_items.entry("oridecon".to_string()).or_insert(0) += 1;
                self.chat_logs.push(format!(
                    "[派遣完了] 遠征部隊が『{}』から無事帰還！ (+{} Zeny, {}x8, オリデオコンx1獲得)",
                    area_name, reward_zeny, drop_item
                ));
            } else {
                self.active_dispatch_progress = Some((area_id, area_name, new_cur, total));
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

        // 弾道パーティクルの更新（移動＆着弾判定）
        let mut completed_particles = Vec::new();
        for (i, p) in self.trajectory_particles.iter_mut().enumerate() {
            p.progress += p.speed * dt;
            if p.progress >= 1.0 {
                completed_particles.push(i);
            }
        }

        // 到達した弾道パーティクルの着弾エフェクト発動
        for idx in completed_particles.into_iter().rev() {
            let p = self.trajectory_particles.remove(idx);
            let (tx, ty) = self.hex_to_screen(p.target_hex);

            if p.is_aoe {
                self.visual_effects.push(VisualEffect {
                    from_x: tx,
                    from_y: ty,
                    to_x: tx,
                    to_y: ty,
                    kind: EffectKind::AoEBlast {
                        radius_hexes: p.aoe_radius,
                        color: Color::new(1.0, 0.4, 0.1, 0.75),
                    },
                    lifetime: 0.45,
                    max_lifetime: 0.45,
                });
            } else {
                self.visual_effects.push(VisualEffect {
                    from_x: tx,
                    from_y: ty,
                    to_x: tx,
                    to_y: ty,
                    kind: EffectKind::Slash { color: p.color },
                    lifetime: 0.25,
                    max_lifetime: 0.25,
                });
            }

            if let Some((text, color, is_big)) = p.damage_text {
                self.floating_texts.push(FloatingText {
                    text,
                    x: tx - 15.0,
                    y: ty - 25.0,
                    color,
                    lifetime: 1.2,
                    max_lifetime: 1.2,
                    is_big,
                });
            }
        }

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
                    let lvs = Progression::grant_exp(member, exp_each, self.next_growth_plan, &mut exp_logs);
                    if lvs > 0 {
                        self.chat_logs.push(format!("[★レベルアップ] {} は Lv.{} に到達！", member.name, member.level));
                    }
                }
                if !party.is_empty() {
                    self.world.active_character = party.remove(0);
                    self.world.party_members = party;
                }
            }
        }
    }

    /// 戦闘イベントを弾道パーティクルや視覚エフェクトに変換
    fn process_visual_events(&mut self, events: Vec<BattleVisualEvent>) {
        for ev in events {
            match ev {
                BattleVisualEvent::DamageDealt {
                    attacker_id,
                    target_id: _,
                    target_hex,
                    amount,
                    is_crit,
                    skill_name,
                } => {
                    let (text, color, is_big) = if let Some(ref sk) = skill_name {
                        (format!("{}! -{}", sk, amount), Color::new(1.0, 0.7, 0.2, 1.0), true)
                    } else if is_crit {
                        (format!("CRIT! -{}", amount), Color::new(1.0, 0.85, 0.1, 1.0), true)
                    } else {
                        (format!("-{}", amount), Color::new(0.95, 0.95, 0.95, 1.0), false)
                    };

                    // 攻撃者の位置を探索
                    let attacker_pos = if let Some(ref s) = self.session {
                        s.team_a.iter().chain(s.team_b.iter())
                            .find(|u| u.character.id == attacker_id)
                            .map(|u| u.position)
                    } else {
                        None
                    };

                    let is_ranged = attacker_pos.map(|pos| HexCoord::distance(&pos, target_hex) >= 2).unwrap_or(false);

                    if let (true, Some(from_pos)) = (is_ranged, attacker_pos) {
                        // 遠距離攻撃または遠距離スキルの弾道パーティクル発射
                        let (fx, fy) = self.hex_to_screen(from_pos);
                        let (tx, ty) = self.hex_to_screen(target_hex);
                        let is_magic = skill_name.is_some();

                        self.trajectory_particles.push(TrajectoryParticle {
                            from_x: fx,
                            from_y: fy - 8.0,
                            to_x: tx,
                            to_y: ty - 8.0,
                            progress: 0.0,
                            speed: if is_magic { 2.8 } else { 3.8 },
                            color: if is_magic { Color::new(1.0, 0.5, 0.15, 1.0) } else { Color::new(0.95, 0.95, 0.7, 1.0) },
                            is_arrow: !is_magic,
                            target_hex,
                            damage_text: Some((text, color, is_big)),
                            is_aoe: is_magic,
                            aoe_radius: 1,
                        });
                    } else {
                        // 近接攻撃：即座にダメージポップアップと斬撃描画
                        let (tx, ty) = self.hex_to_screen(target_hex);
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
                    // 絵文字が含まれる場合は安全な記号に置換
                    let sanitized = msg
                        .replace('⚔', ">")
                        .replace('🛡', "#")
                        .replace('🧙', "@")
                        .replace('🎉', "*")
                        .replace('💥', "!")
                        .replace('💨', "~");
                    self.chat_logs.push(sanitized);
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

        // 3. 右側パネル：マルチタブ・ギルド運営システム
        self.draw_dashboard();

        // 4. 精錬ポップアップおよび全画面フラッシュ
        self.draw_refine_popup();
    }

    /// 2.5Dクォータービュー戦場とユニットの描画
    fn draw_battlefield(&mut self) {
        let mouse_pos = mouse_position();
        let hovered_hex = self.screen_to_hex(mouse_pos.0, mouse_pos.1);
        let in_viewport = mouse_pos.0 < 830.0 && mouse_pos.1 < 535.0;

        // マウス左クリックでユニット選択
        if is_mouse_button_pressed(MouseButton::Left) && in_viewport {
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

            // 戦場タイトルとTick表示
            self.draw_text_helper(
                &format!("[MAP] 戦場: 『{}』 (Tick: {}) | ズーム: {:.1}x", map.name, session.current_tick, self.camera.zoom),
                20.0,
                30.0,
                16.0,
                Color::new(0.85, 0.92, 1.0, 0.95),
            );

            let eff_size = self.hex_size * self.camera.zoom;

            // 1. ヘクスタイル床の描画
            for r in map.min_r..=map.max_r {
                for q in map.min_q..=map.max_q {
                    let coord = HexCoord::new(q, r);
                    let (cx, cy) = self.hex_to_screen(coord);

                    if cx < -60.0 || cx > 890.0 || cy < -60.0 || cy > 560.0 {
                        continue;
                    }

                    let is_blocked = map.blocked_hexes.contains(&coord);
                    let is_hover = coord == hovered_hex && in_viewport;

                    if !is_blocked {
                        let tile_color = if is_hover {
                            Color::new(0.24, 0.36, 0.44, 0.9)
                        } else {
                            Color::new(0.12, 0.16, 0.20, 0.85)
                        };
                        self.draw_flat_hex(cx, cy, eff_size, tile_color, Color::new(0.18, 0.26, 0.34, 0.6));
                    }
                }
            }

            // 2. 選択中ユニットの攻撃射程ハイライト（薄い緑色の六角形塗り＆枠線）
            if let Some((is_team_a, idx)) = self.selected_unit {
                let unit_opt = if is_team_a {
                    session.team_a.get(idx)
                } else {
                    session.team_b.get(idx)
                };
                if let Some(u) = unit_opt {
                    if u.is_alive {
                        for r in map.min_r..=map.max_r {
                            for q in map.min_q..=map.max_q {
                                let target_coord = HexCoord::new(q, r);
                                if !map.blocked_hexes.contains(&target_coord) {
                                    let dist = HexCoord::distance(&u.position, target_coord);
                                    if dist > 0 && dist <= u.attack_range {
                                        let (rx, ry) = self.hex_to_screen(target_coord);
                                        self.draw_flat_hex(
                                            rx,
                                            ry,
                                            eff_size,
                                            Color::new(0.15, 0.65, 0.45, 0.22),
                                            Color::new(0.25, 0.85, 0.55, 0.55),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 3. 障害物・建物の2.5D立体直方体描画
            let wall_height = 20.0 * self.camera.zoom;
            for r in map.min_r..=map.max_r {
                for q in map.min_q..=map.max_q {
                    let coord = HexCoord::new(q, r);
                    if map.blocked_hexes.contains(&coord) {
                        let (cx, cy) = self.hex_to_screen(coord);
                        if cx >= -60.0 && cx <= 890.0 && cy >= -60.0 && cy <= 560.0 {
                            self.draw_25d_building_block(cx, cy, eff_size, wall_height);
                        }
                    }
                }
            }

            // 4. ホバーヘクスハイライト
            if in_viewport {
                let (hcx, hcy) = self.hex_to_screen(hovered_hex);
                if hcx >= -20.0 && hcx <= 850.0 && hcy >= -20.0 && hcy <= 550.0 {
                    self.draw_hex_outline(hcx, hcy, eff_size, Color::new(0.3, 0.8, 1.0, 0.85), 2.0);
                }
            }

            // 5. ユニットの描画（Y座標でソートして手前から奥へ重なり順を正しく描画）
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

            // ユニット本体とコンパクトHPバーを描画
            let mut hovered_unit = None;
            for (is_team_a, idx, unit) in &all_units {
                let is_hover = unit.position == hovered_hex && in_viewport;
                if is_hover {
                    hovered_unit = Some((*is_team_a, *idx, *unit));
                }
                self.draw_unit(unit, *is_team_a, *idx, session.is_monster_battle, is_hover);
            }

            // 6. 選択中またはホバー中のユニットの詳細ネームプレートを最前面にポップアップ表示
            let active_detail_unit = if let Some(h) = hovered_unit {
                Some(h)
            } else if let Some((is_team_a, idx)) = self.selected_unit {
                let u_opt = if is_team_a { session.team_a.get(idx) } else { session.team_b.get(idx) };
                u_opt.map(|u| (is_team_a, idx, u))
            } else {
                None
            };

            if let Some((is_team_a, _, unit)) = active_detail_unit {
                self.draw_smart_nameplate(unit, is_team_a);
            }

            // 7. 弾道パーティクルの描画（矢や魔法球）
            for p in &self.trajectory_particles {
                let cur_x = p.from_x + (p.to_x - p.from_x) * p.progress;
                let cur_y = p.from_y + (p.to_y - p.from_y) * p.progress;

                if p.is_arrow {
                    // 矢の描画（ラインと先端チップ）
                    let tail_x = p.from_x + (p.to_x - p.from_x) * (p.progress - 0.12).max(0.0);
                    let tail_y = p.from_y + (p.to_y - p.from_y) * (p.progress - 0.12).max(0.0);
                    draw_line(tail_x, tail_y, cur_x, cur_y, 2.0, p.color);
                    draw_circle(cur_x, cur_y, 2.5, WHITE);
                } else {
                    // 魔法球の描画（発光リングとコア）
                    draw_circle(cur_x, cur_y, 5.0 * self.camera.zoom, p.color);
                    draw_circle(cur_x, cur_y, 2.5 * self.camera.zoom, WHITE);
                }
            }

            // 8. 視覚エフェクトの描画（レーザー、爆炎、斬撃）
            for ve in &self.visual_effects {
                let alpha = (ve.lifetime / ve.max_lifetime).clamp(0.0, 1.0);
                match ve.kind {
                    EffectKind::LaserLine { color } => {
                        let c = Color::new(color.r, color.g, color.b, color.a * alpha);
                        draw_line(ve.from_x, ve.from_y, ve.to_x, ve.to_y, 3.0, c);
                    }
                    EffectKind::AoEBlast { radius_hexes, color } => {
                        let rad = (radius_hexes as f32 + 0.8) * eff_size * 1.5;
                        let c = Color::new(color.r, color.g, color.b, color.a * alpha);
                        draw_circle(ve.to_x, ve.to_y, rad, c);
                        draw_circle_lines(ve.to_x, ve.to_y, rad, 2.0, Color::new(1.0, 0.9, 0.5, alpha));
                    }
                    EffectKind::Slash { color } => {
                        let c = Color::new(color.r, color.g, color.b, color.a * alpha);
                        let s = 12.0 * self.camera.zoom;
                        draw_line(ve.to_x - s, ve.to_y - s, ve.to_x + s, ve.to_y + s, 2.5, c);
                        draw_line(ve.to_x + s, ve.to_y - s, ve.to_x - s, ve.to_y + s, 2.5, c);
                    }
                    EffectKind::ScreenFlash { color } => {
                        let c = Color::new(color.r, color.g, color.b, color.a * alpha * 0.35);
                        draw_rectangle(0.0, 0.0, 830.0, 535.0, c);
                    }
                }
            }

            // 9. フローティングテキストの描画
            for ft in &self.floating_texts {
                let alpha = (ft.lifetime / ft.max_lifetime).clamp(0.0, 1.0);
                let c = Color::new(ft.color.r, ft.color.g, ft.color.b, alpha);
                let size = if ft.is_big { 16.0 } else { 13.0 };
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

        let side_color_dark = Color::new(0.20, 0.23, 0.28, 1.0);
        let side_color_light = Color::new(0.28, 0.32, 0.38, 1.0);

        // 南東面
        draw_triangle(bottom[1], bottom[2], top[2], side_color_light);
        draw_triangle(bottom[1], top[2], top[1], side_color_light);

        // 南面
        draw_triangle(bottom[2], bottom[3], top[3], side_color_dark);
        draw_triangle(bottom[2], top[3], top[2], side_color_dark);

        // 南西面
        draw_triangle(bottom[3], bottom[4], top[4], side_color_dark);
        draw_triangle(bottom[3], top[4], top[3], side_color_dark);

        // 上面（天板）
        let top_color = Color::new(0.38, 0.44, 0.52, 1.0);
        for k in 1..5 {
            draw_triangle(top[0], top[k], top[k + 1], top_color);
        }

        // 上面枠線
        for k in 0..6 {
            let next = (k + 1) % 6;
            draw_line(top[k].x, top[k].y, top[next].x, top[next].y, 1.2, Color::new(0.50, 0.58, 0.68, 1.0));
        }
    }

    /// ユニット（キャラクター / モンスター）の描画（密集対策：通常時は最小構成）
    fn draw_unit(&self, unit: &Combatant, is_team_a: bool, idx: usize, is_monster_battle: bool, is_hover: bool) {
        let (cx, cy) = self.hex_to_screen(unit.position);
        let eff_size = self.hex_size * self.camera.zoom;

        // 扁平な影
        draw_circle(cx, cy + 2.0, eff_size * 0.50, Color::new(0.0, 0.0, 0.0, 0.45));

        // トークン色設定
        let is_selected = self.selected_unit == Some((is_team_a, idx));
        let is_leech_target = is_team_a && unit.character.level < 15; // 養殖対象Lv

        let (body_color, border_color) = if is_team_a {
            if is_leech_target {
                (Color::new(0.92, 0.78, 0.18, 1.0), Color::new(1.0, 1.0, 0.6, 1.0)) // 養殖キャラ（金色）
            } else {
                (Color::new(0.18, 0.48, 0.85, 1.0), Color::new(0.55, 0.78, 1.0, 1.0)) // 味方（青色）
            }
        } else {
            if is_monster_battle {
                (Color::new(0.82, 0.22, 0.28, 1.0), Color::new(1.0, 0.45, 0.45, 1.0)) // モンスター（赤色）
            } else {
                (Color::new(0.78, 0.38, 0.12, 1.0), Color::new(1.0, 0.60, 0.25, 1.0)) // GvGライバル（橙色）
            }
        };

        // トークン本体円盤
        let unit_radius = 11.5 * self.camera.zoom;
        let unit_y = cy - 7.0 * self.camera.zoom;
        draw_circle(cx, unit_y, unit_radius, body_color);
        draw_circle_lines(
            cx,
            unit_y,
            unit_radius,
            if is_selected || is_hover { 2.2 } else { 1.2 },
            if is_selected { YELLOW } else if is_hover { WHITE } else { border_color },
        );

        // トークン内識別子（JIS記号または1文字目）
        let symbol = if is_leech_target {
            "★".to_string()
        } else {
            unit.character.name.chars().next().unwrap_or('?').to_string()
        };
        let sym_size = (11.0 * self.camera.zoom).clamp(8.0, 14.0);
        self.draw_text_helper(&symbol, cx - sym_size * 0.35, unit_y + sym_size * 0.35, sym_size, if is_leech_target { BLACK } else { WHITE });

        // 通常頭上：コンパクトHPバー（重なり防止のため幅24px、高さ3pxにスリム化）
        let bar_w = 26.0 * self.camera.zoom;
        let bar_h = 3.0 * self.camera.zoom;
        let bar_x = cx - bar_w / 2.0;
        let bar_y = unit_y - 12.0 * self.camera.zoom;

        let hp_ratio = (unit.stats.current_hp.max(0) as f32 / unit.stats.max_hp.max(1) as f32).clamp(0.0, 1.0);
        draw_rectangle(bar_x, bar_y, bar_w, bar_h, Color::new(0.15, 0.15, 0.15, 0.85));
        let hp_color = if hp_ratio > 0.5 {
            Color::new(0.2, 0.85, 0.3, 1.0)
        } else if hp_ratio > 0.2 {
            Color::new(0.9, 0.8, 0.1, 1.0)
        } else {
            Color::new(0.9, 0.2, 0.2, 1.0)
        };
        draw_rectangle(bar_x, bar_y, bar_w * hp_ratio, bar_h, hp_color);
    }

    /// ホバー・選択中のスマート・ネームプレート（最前面ポップアップ）
    fn draw_smart_nameplate(&self, unit: &Combatant, is_team_a: bool) {
        let (cx, cy) = self.hex_to_screen(unit.position);
        let pw = 150.0;
        let ph = 46.0;
        let px = (cx - pw / 2.0).clamp(10.0, 670.0);
        let py = cy - 62.0 * self.camera.zoom;

        draw_rectangle(px, py, pw, ph, Color::new(0.06, 0.09, 0.13, 0.94));
        draw_rectangle_lines(
            px,
            py,
            pw,
            ph,
            1.5,
            if is_team_a { Color::new(0.3, 0.7, 1.0, 0.9) } else { Color::new(1.0, 0.4, 0.4, 0.9) },
        );

        // 1行目: Lv.X 名前 (職種)
        let title = format!("Lv.{} {} [{}]", unit.character.level, unit.character.name, unit.character.job_id);
        self.draw_text_helper(&title, px + 8.0, py + 16.0, 12.0, WHITE);

        // 2行目: HPバーと数値
        let bar_x = px + 8.0;
        let bar_y = py + 22.0;
        let bar_w = pw - 16.0;
        let bar_h = 6.0;
        let hp_ratio = (unit.stats.current_hp.max(0) as f32 / unit.stats.max_hp.max(1) as f32).clamp(0.0, 1.0);
        draw_rectangle(bar_x, bar_y, bar_w, bar_h, Color::new(0.2, 0.2, 0.2, 0.9));
        draw_rectangle(bar_x, bar_y, bar_w * hp_ratio, bar_h, Color::new(0.2, 0.85, 0.35, 1.0));

        // 3行目: HP現在値 / 最大値 / 射程
        let hp_text = format!("HP: {}/{} | 射程: {}", unit.stats.current_hp, unit.stats.max_hp, unit.attack_range);
        self.draw_text_helper(&hp_text, px + 8.0, py + 40.0, 10.0, Color::new(0.8, 0.9, 1.0, 0.9));
    }

    /// 左下パネル：RO風チャット＆システムログウィンドウ
    fn draw_chat_panel(&self) {
        let x = 15.0;
        let y = 540.0;
        let w = 810.0;
        let h = 240.0;

        draw_rectangle(x, y, w, h, Color::new(0.05, 0.07, 0.10, 0.92));
        draw_rectangle_lines(x, y, w, h, 1.5, Color::new(0.25, 0.35, 0.48, 0.85));

        self.draw_text_helper("[ギルド通信 ＆ 戦闘システムログ]", x + 15.0, y + 20.0, 13.0, Color::new(0.4, 0.75, 1.0, 1.0));

        let start_idx = if self.chat_logs.len() > 10 {
            self.chat_logs.len() - 10
        } else {
            0
        };

        for (i, log) in self.chat_logs[start_idx..].iter().enumerate() {
            let line_y = y + 42.0 + (i as f32 * 18.5);
            let log_color = if log.contains("クホホ") || log.contains("砕け") || log.contains("全滅") || log.contains("敗北") {
                Color::new(1.0, 0.40, 0.50, 1.0) // 警告・ロスト・死亡
            } else if log.contains("レベルアップ") || log.contains("成功") || log.contains("帰還") || log.contains("大成功") {
                Color::new(0.3, 0.95, 0.6, 1.0) // 歓喜・成功
            } else if log.contains("Tick") {
                Color::new(0.85, 0.88, 0.92, 0.9) // 戦闘ステップ
            } else {
                Color::new(0.70, 0.85, 0.95, 0.95) // システムメッセージ
            };

            self.draw_text_helper(log, x + 15.0, line_y, 12.0, log_color);
        }
    }

    /// 右側パネル：マルチタブ・ギルド運営システム
    fn draw_dashboard(&mut self) {
        let x = 835.0;
        let y = 15.0;
        let w = 430.0;
        let h = 765.0;

        draw_rectangle(x, y, w, h, Color::new(0.08, 0.11, 0.16, 0.96));
        draw_rectangle_lines(x, y, w, h, 2.0, Color::new(0.35, 0.48, 0.65, 0.9));

        // ギルド概要ヘッダー
        let total_party_count = 1 + self.world.party_members.len();
        self.draw_text_helper("[◆ ギルド司令部: Realm of Origins]", x + 16.0, y + 24.0, 16.0, Color::new(1.0, 0.85, 0.3, 1.0));
        self.draw_text_helper(
            &format!("世代: 第{}世代 | 資金: {} Zeny | 主力: {}名 | 倉庫: {}種",
                self.world.generation, self.world.treasury_zeny, total_party_count, self.world.storage_items.len()
            ),
            x + 16.0,
            y + 45.0,
            12.5,
            Color::new(0.8, 0.9, 1.0, 0.95),
        );

        draw_line(x + 12.0, y + 56.0, x + w - 12.0, y + 56.0, 1.0, Color::new(0.3, 0.4, 0.5, 0.6));

        // 4つの上部タブボタン
        let tab_w = 98.0;
        let tab_h = 26.0;
        let tab_y = y + 64.0;
        let base_tab_color = Color::new(0.16, 0.22, 0.30, 1.0);

        if self.draw_button(Rect::new(x + 14.0, tab_y, tab_w, tab_h), "[1:戦場/編成]", self.active_tab == DashboardTab::Battlefield, base_tab_color) {
            self.active_tab = DashboardTab::Battlefield;
        }
        if self.draw_button(Rect::new(x + 116.0, tab_y, tab_w, tab_h), "[2:工房/精錬]", self.active_tab == DashboardTab::Workshop, base_tab_color) {
            self.active_tab = DashboardTab::Workshop;
        }
        if self.draw_button(Rect::new(x + 218.0, tab_y, tab_w, tab_h), "[3:市場/派遣]", self.active_tab == DashboardTab::MarketDispatch, base_tab_color) {
            self.active_tab = DashboardTab::MarketDispatch;
        }
        if self.draw_button(Rect::new(x + 320.0, tab_y, tab_w, tab_h), "[4:名簿/転生]", self.active_tab == DashboardTab::RosterLegacy, base_tab_color) {
            self.active_tab = DashboardTab::RosterLegacy;
        }

        draw_line(x + 12.0, tab_y + tab_h + 8.0, x + w - 12.0, tab_y + tab_h + 8.0, 1.0, Color::new(0.25, 0.35, 0.45, 0.6));

        let content_y = tab_y + tab_h + 16.0;

        // タブごとの画面描画
        match self.active_tab {
            DashboardTab::Battlefield => self.draw_tab_battlefield(x + 16.0, content_y, w - 32.0),
            DashboardTab::Workshop => self.draw_tab_workshop(x + 16.0, content_y, w - 32.0),
            DashboardTab::MarketDispatch => self.draw_tab_market_dispatch(x + 16.0, content_y, w - 32.0),
            DashboardTab::RosterLegacy => self.draw_tab_roster_legacy(x + 16.0, content_y, w - 32.0),
        }
    }

    /// タブ1: 戦場/編成（マップ選択・速度・武器持ち替え・ガンビット切替・ロール変更）
    fn draw_tab_battlefield(&mut self, sx: f32, sy: f32, sw: f32) {
        let mut y = sy;

        // 1. マップ選択と戦闘開始
        self.draw_text_helper("◆ 作戦戦場の選択 ＆ 展開:", sx, y + 14.0, 13.5, Color::new(0.9, 0.75, 0.3, 1.0));
        y += 22.0;

        // 『監視塔の隘路』
        if self.draw_button(Rect::new(sx, y, sw, 26.0), "◆ 監視塔の隘路防衛 ＆ 養殖 (幅1ヘクス狭路)", false, Color::new(0.16, 0.28, 0.42, 1.0)) {
            let engine = SimulationEngine::new(&self.db);
            let map = HexMap::create_chokepoint_map();
            let party = self.get_player_party();
            let t_orc = self.db.monsters.get("high_orc").expect("high_orc exists");
            let t_swift = self.db.monsters.get("orc_warrior").expect("orc_warrior exists");
            let m1 = engine.spawn_monster(t_orc, MonsterArchetype::Heavy);
            let m2 = engine.spawn_monster(t_swift, MonsterArchetype::Swift);
            self.session = Some(engine.create_chokepoint_session(map, &party, &[m1, m2]));
            self.chat_logs.push("[戦況] 『監視塔の隘路』防衛バトルを開始！ 前衛が隘路を封鎖します。".to_string());
        }
        y += 30.0;

        // 『廃坑深層』
        if self.draw_button(Rect::new(sx, y, sw, 26.0), "◆ 廃坑深層・坑道迎撃戦 (巨岩と障害物)", false, Color::new(0.24, 0.22, 0.38, 1.0)) {
            let engine = SimulationEngine::new(&self.db);
            let map = HexMap::create_mine_depths_map();
            let party = self.get_player_party();
            let t_orc = self.db.monsters.get("high_orc").expect("high_orc exists");
            let m1 = engine.spawn_monster(t_orc, MonsterArchetype::Heavy);
            let m2 = engine.spawn_monster(t_orc, MonsterArchetype::Swift);
            let m3 = engine.spawn_monster(t_orc, MonsterArchetype::Heavy);
            self.session = Some(engine.create_chokepoint_session(map, &party, &[m1, m2, m3]));
            self.chat_logs.push("[戦況] 『廃坑深層』坑道迎撃戦を開始！ 狭い岩石地帯で激突します。".to_string());
        }
        y += 30.0;

        // 『栄光の騎士団闘技場 (GvG)』
        if self.draw_button(Rect::new(sx, y, sw, 26.0), "★ 空間GvG模擬戦：宿敵『黒曜の旅団』激突", false, Color::new(0.35, 0.20, 0.32, 1.0)) {
            let engine = SimulationEngine::new(&self.db);
            let map = HexMap::create_gvg_arena_map();
            let party = self.get_player_party();
            let rival_party = if let Some(rival) = self.world.rival_guilds.iter().find(|g| g.name.contains("黒曜")) {
                rival.roster.clone()
            } else {
                party.clone()
            };
            self.session = Some(engine.create_gvg_session(map, &party, &rival_party, "自軍ギルド", "黒曜の旅団", None));
            self.chat_logs.push("[戦況] 空間GvG模擬戦が開幕！ 栄光の闘技場で宿敵と激突します。".to_string());
        }
        y += 36.0;

        // 速度切替＆カメラリセット
        self.draw_text_helper("速度 ＆ カメラ:", sx, y + 14.0, 13.0, WHITE);
        let btn_w = 54.0;
        let mut bx = sx + 90.0;
        if self.draw_button(Rect::new(bx, y, btn_w, 22.0), "停止", self.play_speed == PlaySpeed::Paused, Color::new(0.2, 0.25, 0.32, 1.0)) {
            self.play_speed = PlaySpeed::Paused;
        }
        bx += btn_w + 6.0;
        if self.draw_button(Rect::new(bx, y, btn_w, 22.0), "1x", self.play_speed == PlaySpeed::Normal, Color::new(0.2, 0.25, 0.32, 1.0)) {
            self.play_speed = PlaySpeed::Normal;
        }
        bx += btn_w + 6.0;
        if self.draw_button(Rect::new(bx, y, btn_w, 22.0), "3x", self.play_speed == PlaySpeed::Fast, Color::new(0.2, 0.25, 0.32, 1.0)) {
            self.play_speed = PlaySpeed::Fast;
        }
        bx += btn_w + 6.0;
        if self.draw_button(Rect::new(bx, y, btn_w, 22.0), "MAX", self.play_speed == PlaySpeed::Instant, Color::new(0.2, 0.25, 0.32, 1.0)) {
            self.play_speed = PlaySpeed::Instant;
        }
        bx += btn_w + 8.0;
        if self.draw_button(Rect::new(bx, y, 62.0, 22.0), "Reset", false, Color::new(0.28, 0.24, 0.20, 1.0)) {
            self.reset_camera();
        }
        y += 30.0;

        draw_line(sx, y, sx + sw, y, 1.0, Color::new(0.3, 0.4, 0.5, 0.5));
        y += 8.0;

        // 2. 選択中ユニットのステータスとインタラクティブ編成
        self.draw_text_helper("◆ ユニット編成 ＆ 作戦カスタマイズ:", sx, y + 14.0, 13.5, Color::new(0.9, 0.9, 0.5, 1.0));
        y += 22.0;

        let unit_info = if let Some(ref session) = self.session {
            let u_opt = if let Some((is_team_a, idx)) = self.selected_unit {
                if is_team_a { session.team_a.get(idx) } else { session.team_b.get(idx) }
            } else {
                session.team_a.first()
            };
            u_opt.map(|u| {
                (
                    u.character.id.clone(),
                    u.character.name.clone(),
                    u.character.job_id.clone(),
                    u.character.level,
                    u.attack_range,
                    u.stats.current_hp,
                    u.stats.max_hp,
                    u.stats.min_atk,
                    u.stats.max_atk,
                    u.stats.equip_def,
                    u.stats.vit_def,
                    u.stats.hit,
                    u.stats.flee,
                    u.stats.crit,
                    u.stats.aspd,
                    u.character.formation.role,
                )
            })
        } else {
            None
        };

        if let Some((
            char_id,
            char_name,
            char_job,
            char_level,
            attack_range,
            cur_hp,
            max_hp,
            min_atk,
            max_atk,
            equip_def,
            vit_def,
            hit,
            flee,
            crit,
            aspd,
            role,
        )) = unit_info
        {
            self.draw_text_helper(&format!("名前: {} ({}) | Lv.{} | 射程: {}", char_name, char_job, char_level, attack_range), sx, y + 14.0, 13.0, WHITE);
            y += 18.0;
            self.draw_text_helper(&format!("HP: {}/{} | ATK: {}〜{} | DEF: {}+{}", cur_hp, max_hp, min_atk, max_atk, equip_def, vit_def), sx, y + 14.0, 11.5, Color::new(0.8, 0.9, 1.0, 0.9));
            y += 18.0;
            self.draw_text_helper(&format!("HIT: {} | FLEE: {} | CRIT: {:.1}% | ASPD: {:.1}", hit, flee, crit, aspd), sx, y + 14.0, 11.5, Color::new(0.8, 0.9, 1.0, 0.9));
            y += 24.0;

            // 主武器の直接持ち替えボタン（短剣 / 長弓 / 槍 / 片手剣）
            self.draw_text_helper("主武器の持ち替え:", sx, y + 12.0, 12.0, Color::new(1.0, 0.8, 0.4, 1.0));
            y += 18.0;

            let w_btn_w = (sw - 12.0) / 4.0;
            let weapons = [
                ("短剣 (射程1)", "dagger", 1, 50),
                ("長弓 (射程4)", "bow", 4, 75),
                ("槍 (射程2)", "spear", 2, 110),
                ("剣 (射程1)", "sword", 1, 100),
            ];

            for (i, (w_label, w_type, range, atk)) in weapons.iter().enumerate() {
                let wx = sx + i as f32 * (w_btn_w + 4.0);
                if self.draw_button(Rect::new(wx, y, w_btn_w, 24.0), w_label, attack_range == *range, Color::new(0.20, 0.26, 0.35, 1.0)) {
                    // 武器の換装
                    let new_eq = Equipment {
                        id: format!("weap_{}", w_type),
                        name: w_label.to_string(),
                        slot: EquipSlot::Weapon,
                        weapon_atk: *atk,
                        attack_range: *range,
                        refine: 4,
                        slots: 4,
                        cards: vec![],
                        modifiers: vec![],
                        is_locked: false,
                    };

                    // 主力キャラクターまたは同行メンバーの装備を更新
                    if self.world.active_character.id == char_id {
                        self.world.active_character.equipments.retain(|e| e.slot != EquipSlot::Weapon);
                        self.world.active_character.equipments.push(new_eq.clone());
                    } else if let Some(mem) = self.world.party_members.iter_mut().find(|m| m.id == char_id) {
                        mem.equipments.retain(|e| e.slot != EquipSlot::Weapon);
                        mem.equipments.push(new_eq.clone());
                    }

                    // セッション中の戦闘ユニットも即時更新
                    if let Some(ref mut session) = self.session {
                        if let Some(combatant) = session.team_a.iter_mut().find(|u| u.character.id == char_id) {
                            combatant.attack_range = *range;
                            combatant.character.equipments.retain(|e| e.slot != EquipSlot::Weapon);
                            combatant.character.equipments.push(new_eq);
                        }
                    }
                    self.chat_logs.push(format!("[装備換装] {} は 『{}』 に持ち替えました！ (射程: {})", char_name, w_label, range));
                }
            }
            y += 32.0;

            // ガンビット作戦プリセット切替
            self.draw_text_helper("ガンビット戦術プリセット切替:", sx, y + 12.0, 12.0, Color::new(0.5, 0.9, 0.7, 1.0));
            y += 18.0;

            let g_btn_w = (sw - 12.0) / 4.0;
            let gambit_presets = [
                ("標準作戦", 0),
                ("対Swift必中", 1),
                ("対高VIT錐", 2),
                ("密集AoE爆撃", 3),
            ];

            for (i, (g_label, mode)) in gambit_presets.iter().enumerate() {
                let gx = sx + i as f32 * (g_btn_w + 4.0);
                if self.draw_button(Rect::new(gx, y, g_btn_w, 24.0), g_label, false, Color::new(0.18, 0.28, 0.25, 1.0)) {
                    let new_rules = match mode {
                        1 => vec![crate::models::GambitRule {
                            id: "swift_killer".to_string(),
                            priority: 1,
                            condition: Condition::TargetArchetypeIs(MonsterArchetype::Swift),
                            action: Action::UseActiveSkill("double_strafe".to_string()),
                        }],
                        2 => vec![crate::models::GambitRule {
                            id: "vit_piercer".to_string(),
                            priority: 1,
                            condition: Condition::TargetVitAbove(35),
                            action: Action::UseActiveSkill("bash".to_string()),
                        }],
                        3 => vec![crate::models::GambitRule {
                            id: "cluster_blast".to_string(),
                            priority: 1,
                            condition: Condition::EnemiesInRadius { radius: 2, min_count: 2 },
                            action: Action::UseActiveSkill("fire_ball".to_string()),
                        }],
                        _ => vec![],
                    };

                    if self.world.active_character.id == char_id {
                        self.world.active_character.gambit_rules = new_rules.clone();
                    } else if let Some(mem) = self.world.party_members.iter_mut().find(|m| m.id == char_id) {
                        mem.gambit_rules = new_rules.clone();
                    }

                    if let Some(ref mut session) = self.session {
                        if let Some(combatant) = session.team_a.iter_mut().find(|u| u.character.id == char_id) {
                            combatant.character.gambit_rules = new_rules;
                        }
                    }
                    self.chat_logs.push(format!("[作戦変更] {} のガンビットを 『{}』 に切り替えました！", char_name, g_label));
                }
            }
            y += 32.0;

            // 陣形ロール変更（前衛 / 中衛 / 後衛）
            self.draw_text_helper("陣形配置ロール:", sx, y + 12.0, 12.0, Color::new(0.6, 0.85, 1.0, 1.0));
            y += 18.0;

            let r_btn_w = (sw - 8.0) / 3.0;
            let roles = [
                ("前衛 (Frontline)", FormationRole::Frontline),
                ("中衛 (Midline)", FormationRole::Midline),
                ("後衛 (Backline)", FormationRole::Backline),
            ];

            for (i, (r_label, r_val)) in roles.iter().enumerate() {
                let rx = sx + i as f32 * (r_btn_w + 4.0);
                let is_cur = role == *r_val;
                if self.draw_button(Rect::new(rx, y, r_btn_w, 24.0), r_label, is_cur, Color::new(0.20, 0.28, 0.38, 1.0)) {
                    if self.world.active_character.id == char_id {
                        self.world.active_character.formation.role = *r_val;
                    } else if let Some(mem) = self.world.party_members.iter_mut().find(|m| m.id == char_id) {
                        mem.formation.role = *r_val;
                    }
                    if let Some(ref mut session) = self.session {
                        if let Some(combatant) = session.team_a.iter_mut().find(|u| u.character.id == char_id) {
                            combatant.character.formation.role = *r_val;
                        }
                    }
                    self.chat_logs.push(format!("[陣形変更] {} の配置を 『{:?}』 に指定しました！", char_name, r_val));
                }
            }
        } else {
            self.draw_text_helper("戦場上のユニットをクリックすると詳細・作戦変更が可能です。", sx, y + 30.0, 12.0, GRAY);
        }
    }

    /// タブ2: 工房/精錬（クホホ工場: 装備選択・手動1回・安全圏一括・+7/+10オーダー・ロック・カード装着）
    fn draw_tab_workshop(&mut self, sx: f32, sy: f32, sw: f32) {
        let mut y = sy;
        self.draw_text_helper("◆ 鍛冶工房: クホホ工場 (過剰精錬＆スロット拡張)", sx, y + 14.0, 14.0, Color::new(1.0, 0.8, 0.3, 1.0));
        y += 24.0;

        // 倉庫内の装備一覧（または主力装備）
        self.draw_text_helper("精錬対象の装備を選択:", sx, y + 12.0, 12.5, WHITE);
        y += 18.0;

        let row_h = 24.0;

        // 主力装備をインデックス 0 として扱い、倉庫品を後続に並べる
        let mut all_equips = Vec::new();
        if let Some(weap) = self.world.active_character.equipments.iter().find(|e| e.slot == EquipSlot::Weapon) {
            all_equips.push((true, 0, weap.clone(), format!("(装備中) +{} {}", weap.refine, weap.name)));
        }
        for (i, eq) in self.world.storage_equipments.iter().enumerate() {
            all_equips.push((false, i, eq.clone(), format!("(倉庫) +{} {} [ATK:{}]", eq.refine, eq.name, eq.weapon_atk)));
        }

        for (item_idx, (_is_equipped, _orig_idx, eq, label)) in all_equips.iter().enumerate().take(5) {
            let is_sel = self.selected_storage_equip_idx == Some(item_idx);
            let lock_tag = if eq.is_locked { " [L]" } else { "" };
            let full_label = format!("{}{}", label, lock_tag);

            if self.draw_button(Rect::new(sx, y, sw, row_h), &full_label, is_sel, Color::new(0.18, 0.22, 0.28, 1.0)) {
                self.selected_storage_equip_idx = Some(item_idx);
            }
            y += row_h + 4.0;
        }

        y += 8.0;
        draw_line(sx, y, sx + sw, y, 1.0, Color::new(0.3, 0.4, 0.5, 0.5));
        y += 8.0;

        // 精錬アクション群
        self.draw_text_helper("◆ 精錬アクション ＆ ワークオーダー:", sx, y + 14.0, 13.0, Color::new(0.9, 0.85, 0.5, 1.0));
        y += 22.0;

        let half_w = (sw - 6.0) / 2.0;

        // 1. 手動で+1精錬 (300 Zeny)
        if self.draw_button(Rect::new(sx, y, half_w, 26.0), "[+1 精錬する (300Z)]", false, Color::new(0.32, 0.22, 0.16, 1.0)) {
            self.execute_single_refine();
        }

        // 2. 安全圏(+4)まで一括
        if self.draw_button(Rect::new(sx + half_w + 6.0, y, half_w, 26.0), "[安全圏(+4)まで一括]", false, Color::new(0.18, 0.32, 0.22, 1.0)) {
            self.execute_safe_refine_to_4();
        }
        y += 32.0;

        // 3. 目標+7ワークオーダー
        if self.draw_button(Rect::new(sx, y, half_w, 26.0), "[目標+7 限界突破]", false, Color::new(0.35, 0.25, 0.15, 1.0)) {
            self.execute_target_order(7);
        }

        // 4. 目標+10神話ワークオーダー
        if self.draw_button(Rect::new(sx + half_w + 6.0, y, half_w, 26.0), "[目標+10 神話突破]", false, Color::new(0.42, 0.15, 0.22, 1.0)) {
            self.execute_target_order(10);
        }
        y += 32.0;

        // 5. ロック切り替え & 6. カード装着
        if self.draw_button(Rect::new(sx, y, half_w, 26.0), "[ロック切替 (誤破壊防止)]", false, Color::new(0.22, 0.26, 0.34, 1.0)) {
            self.toggle_selected_weapon_lock();
        }

        if self.draw_button(Rect::new(sx + half_w + 6.0, y, half_w, 26.0), "[空き枠へカード装着]", false, Color::new(0.28, 0.20, 0.38, 1.0)) {
            self.attach_card_to_selected_weapon();
        }
        y += 36.0;

        // 鍛冶屋ボーナスと安全圏の説明
        let smith_bonus = 8.5f32; // DEX/LUKボーナス
        self.draw_text_helper(&format!("専属鍛冶屋職能ボーナス: +{:.1}% (成功率底上げ中)", smith_bonus), sx, y + 12.0, 11.5, Color::new(0.7, 0.9, 0.8, 0.9));
        y += 18.0;
        self.draw_text_helper("※+4までは安全圏(100%)。+5以降は失敗時に『クホホ…』消滅します。", sx, y + 12.0, 11.0, Color::new(0.85, 0.7, 0.7, 0.8));
    }

    /// タブ3: 市場/派遣（BOM経済＆遠征: 価格・在庫バー・買付・売却・自動遠征）
    fn draw_tab_market_dispatch(&mut self, sx: f32, sy: f32, sw: f32) {
        let mut y = sy;
        self.draw_text_helper("◆ BOM経済連動市場 ＆ 素材相場:", sx, y + 14.0, 14.0, Color::new(0.4, 0.85, 0.9, 1.0));
        y += 24.0;

        // 市場アイテムテーブル
        let items = [
            ("原木 (wood)", "wood", 120),
            ("鉄鉱石 (iron_ore)", "iron_ore", 300),
            ("鋼鉄 (steel)", "steel", 1800),
            ("オリデオコン (oridecon)", "oridecon", 4500),
            ("白ポーション", "white_potion", 600),
        ];

        self.draw_text_helper("品目 / 相場 / 在庫 / 騰落率", sx, y + 10.0, 11.5, GRAY);
        y += 16.0;

        for (label, id, base_price) in items {
            let cur_price = self.world.market.items.get(id).map(|it| it.buy_price()).unwrap_or(base_price);
            let diff_ratio = ((cur_price as f32 - base_price as f32) / base_price as f32) * 100.0;
            let my_stock = self.world.storage_items.get(id).cloned().unwrap_or(0);

            // 品名と現在価格
            let name_text = format!("{} : {} Z", label, cur_price);
            self.draw_text_helper(&name_text, sx, y + 14.0, 12.0, WHITE);

            // 騰落率（プラスは緑、マイナスは赤）
            let (diff_text, diff_color) = if diff_ratio >= 0.0 {
                (format!("(+{:.0}%)", diff_ratio), Color::new(0.3, 0.95, 0.5, 1.0))
            } else {
                (format!("({:.0}%)", diff_ratio), Color::new(1.0, 0.4, 0.4, 1.0))
            };
            self.draw_text_helper(&diff_text, sx + 180.0, y + 14.0, 11.0, diff_color);

            // 倉庫在庫表示
            self.draw_text_helper(&format!("所持: {}", my_stock), sx + 235.0, y + 14.0, 11.0, Color::new(0.7, 0.8, 0.9, 0.85));

            // [買付 x5] ボタン
            if self.draw_button(Rect::new(sx + sw - 105.0, y, 48.0, 20.0), "買付x5", false, Color::new(0.18, 0.28, 0.38, 1.0)) {
                if let Ok(cost) = self.world.market.buy(id, 5) {
                    if self.world.treasury_zeny >= cost {
                        self.world.treasury_zeny -= cost;
                        *self.world.storage_items.entry(id.to_string()).or_insert(0) += 5;
                        self.chat_logs.push(format!("[市場取引] {} を5個買付！ (-{} Zeny, 相場連動中)", label, cost));
                    } else {
                        self.chat_logs.push("[資金不足] Zenyが足りません。".to_string());
                    }
                }
            }

            // [売却 All] ボタン
            if self.draw_button(Rect::new(sx + sw - 52.0, y, 48.0, 20.0), "全売却", false, Color::new(0.32, 0.22, 0.18, 1.0)) {
                if my_stock > 0 {
                    if let Ok(earned) = self.world.market.sell(id, my_stock, 0.0) {
                        self.world.treasury_zeny += earned;
                        self.world.storage_items.insert(id.to_string(), 0);
                        self.chat_logs.push(format!("[市場売却] {} を全{}個売却！ (+{} Zeny獲得)", label, my_stock, earned));
                    }
                }
            }

            y += 25.0;
        }

        y += 8.0;
        draw_line(sx, y, sx + sw, y, 1.0, Color::new(0.3, 0.4, 0.5, 0.5));
        y += 8.0;

        // 控え部隊の自動派遣（遠征）システム
        self.draw_text_helper("◆ 控え部隊の自動派遣 (オートチェス型遠征):", sx, y + 14.0, 13.5, Color::new(0.9, 0.8, 0.4, 1.0));
        y += 24.0;

        let idle_count = self.world.roster.active_members.len();
        self.draw_text_helper(&format!("待機中の控えメンバー: {}名 (派遣可能)", idle_count), sx, y + 12.0, 12.0, Color::new(0.8, 0.9, 1.0, 0.9));
        y += 18.0;

        if let Some((_, ref aname, cur, total)) = self.active_dispatch_progress {
            let prog_ratio = (cur / total).clamp(0.0, 1.0);
            self.draw_text_helper(&format!("遠征中: 『{}』 進行度: {:.0}%", aname, prog_ratio * 100.0), sx, y + 14.0, 12.0, Color::new(0.3, 0.95, 0.6, 1.0));
            y += 18.0;

            let bar_w = sw;
            let bar_h = 8.0;
            draw_rectangle(sx, y, bar_w, bar_h, Color::new(0.2, 0.2, 0.2, 0.8));
            draw_rectangle(sx, y, bar_w * prog_ratio, bar_h, Color::new(0.2, 0.8, 0.4, 1.0));
            draw_rectangle_lines(sx, y, bar_w, bar_h, 1.0, Color::new(0.4, 0.5, 0.6, 0.8));
        } else {
            let half_w = (sw - 6.0) / 2.0;

            if self.draw_button(Rect::new(sx, y, half_w, 28.0), "[廃坑深層 (鉱石採掘)]", false, Color::new(0.22, 0.28, 0.38, 1.0)) {
                self.start_dispatch_expedition("abandoned_mine", "廃坑の深層（鉱山資源採掘）");
            }
            if self.draw_button(Rect::new(sx + half_w + 6.0, y, half_w, 28.0), "[薬草園 (ハーブ採集)]", false, Color::new(0.18, 0.32, 0.25, 1.0)) {
                self.start_dispatch_expedition("herb_forest", "妖精の薬草園（白ハーブ採集）");
            }
        }
    }

    /// タブ4: 名簿/転生（サクセス×ペナント管理: 世代交代・控え名簿・ライバル戦力・FA処遇）
    fn draw_tab_roster_legacy(&mut self, sx: f32, sy: f32, sw: f32) {
        let mut y = sy;
        self.draw_text_helper("◆ レガシー転生 ＆ 世代継承 (サクセス育成):", sx, y + 14.0, 14.0, Color::new(0.9, 0.75, 0.4, 1.0));
        y += 22.0;

        let cur_hero = &self.world.active_character;
        self.draw_text_helper(
            &format!("現役英雄: 第{}世代 {} (Lv.{} {})", cur_hero.generation, cur_hero.name, cur_hero.level, cur_hero.job_id),
            sx,
            y + 12.0,
            12.5,
            WHITE,
        );
        y += 18.0;

        // 次世代のGrowthPlan選択ボタン群
        self.draw_text_helper("次世代ルーキーの育成方針 (GrowthPlan):", sx, y + 12.0, 11.5, Color::new(0.7, 0.85, 1.0, 0.9));
        y += 18.0;

        let plan_btn_w = (sw - 12.0) / 4.0;
        let plans = [
            ("AgiCrit", GrowthPlan::AgiCrit),
            ("VitTank", GrowthPlan::VitTank),
            ("Crafter", GrowthPlan::DexLukCrafter),
            ("Support", GrowthPlan::IntDexSupport),
        ];

        for (i, (plabel, pval)) in plans.iter().enumerate() {
            let px = sx + i as f32 * (plan_btn_w + 4.0);
            let is_sel = self.next_growth_plan == *pval;
            if self.draw_button(Rect::new(px, y, plan_btn_w, 22.0), plabel, is_sel, Color::new(0.20, 0.25, 0.32, 1.0)) {
                self.next_growth_plan = *pval;
            }
        }
        y += 28.0;

        // [レガシー転生実行] ボタン
        if self.draw_button(Rect::new(sx, y, sw, 28.0), ">> レガシー転生実行 (先代を控え名簿へ継承)", false, Color::new(0.38, 0.22, 0.18, 1.0)) {
            self.execute_succession_action();
        }
        y += 36.0;

        draw_line(sx, y, sx + sw, y, 1.0, Color::new(0.3, 0.4, 0.5, 0.5));
        y += 8.0;

        // ライバル3大ギルドの戦力情報
        self.draw_text_helper("◆ ライバル3大ギルド戦力評価:", sx, y + 14.0, 13.0, Color::new(1.0, 0.8, 0.3, 1.0));
        y += 22.0;

        let rivals = [
            ("黒曜の旅団 (宿敵クロード率いる)", "戦力: Sランク (物理特化/過剰精錬)"),
            ("黄金の盾 (鉄壁ディフェンス)", "戦力: Aランク (超高VIT金剛壁)"),
            ("蒼天の魔導会 (広域爆撃)", "戦力: Aランク (詠唱AoE範囲魔法)"),
        ];

        for (rname, rstat) in rivals {
            self.draw_text_helper(&format!("・{} : {}", rname, rstat), sx + 4.0, y + 12.0, 11.5, Color::new(0.85, 0.9, 0.95, 0.9));
            y += 18.0;
        }

        y += 8.0;
        draw_line(sx, y, sx + sw, y, 1.0, Color::new(0.3, 0.4, 0.5, 0.5));
        y += 8.0;

        // FA（フリーエージェント）処遇ボタン
        self.draw_text_helper("◆ FA宣言キャラの処遇操作:", sx, y + 14.0, 13.0, Color::new(0.9, 0.7, 0.8, 1.0));
        y += 22.0;

        let fa_btn_w = (sw - 8.0) / 3.0;
        if self.draw_button(Rect::new(sx, y, fa_btn_w, 24.0), "[引き留め]", false, Color::new(0.20, 0.35, 0.25, 1.0)) {
            if self.world.treasury_zeny >= 5000 {
                self.world.treasury_zeny -= 5000;
                self.chat_logs.push("[FA引き留め] ボーナス5,000Zを支給し、古参メンバーの残留に成功しました！".to_string());
            } else {
                self.chat_logs.push("[資金不足] 引き留めボーナス資金が足りません。".to_string());
            }
        }
        if self.draw_button(Rect::new(sx + fa_btn_w + 4.0, y, fa_btn_w, 24.0), "[街NPC化]", false, Color::new(0.28, 0.24, 0.35, 1.0)) {
            self.chat_logs.push("[街NPC化] 引退メンバーが街の専属鍛冶屋に就任！ (精錬成功率+10%補正)".to_string());
        }
        if self.draw_button(Rect::new(sx + (fa_btn_w + 4.0) * 2.0, y, fa_btn_w, 24.0), "[移籍承認]", false, Color::new(0.38, 0.20, 0.20, 1.0)) {
            self.chat_logs.push("[FA移籍承認] メンバーが宿敵『黒曜の旅団』へFA移籍しました！ 次回GvGで立ちはだかります。".to_string());
        }
        y += 32.0;

        // セーブ＆ロード
        let half_w = (sw - 6.0) / 2.0;
        if self.draw_button(Rect::new(sx, y, half_w, 24.0), "[セーブ (save_data.ron)]", false, Color::new(0.18, 0.25, 0.35, 1.0)) {
            match self.world.save_to_file("saves/save_data.ron") {
                Ok(_) => self.chat_logs.push("[保存完了] 'saves/save_data.ron' へ保存しました。".to_string()),
                Err(e) => self.chat_logs.push(format!("[保存失敗] {}", e)),
            }
        }
        if self.draw_button(Rect::new(sx + half_w + 6.0, y, half_w, 24.0), "[ロード (save_data.ron)]", false, Color::new(0.18, 0.25, 0.35, 1.0)) {
            match WorldState::load_from_file("saves/save_data.ron") {
                Ok(loaded) => {
                    self.world = loaded;
                    self.chat_logs.push("[読込完了] 'saves/save_data.ron' から復元しました。".to_string());
                }
                Err(e) => self.chat_logs.push(format!("[読込失敗] {}", e)),
            }
        }
    }

    /// 手動1回精錬の実行
    fn execute_single_refine(&mut self) {
        let cost = 300u32;
        if self.world.treasury_zeny < cost {
            self.chat_logs.push("[精錬不可] 資金が足りません (必要: 300 Zeny)".to_string());
            return;
        }
        self.world.treasury_zeny -= cost;

        let mut refine_logs = Vec::new();
        let msg;
        let is_success;

        if let Some(eq) = self.world.active_character.equipments.iter_mut().find(|e| e.slot == EquipSlot::Weapon) {
            let old_refine = eq.refine;
            match Refinery::refine_once(eq, None, &mut refine_logs) {
                Ok(true) => {
                    is_success = true;
                    msg = format!("[大成功!] {} の {} が +{} に強化されました！", self.world.active_character.name, eq.name, eq.refine);
                    self.visual_effects.push(VisualEffect {
                        from_x: 0.0,
                        from_y: 0.0,
                        to_x: 0.0,
                        to_y: 0.0,
                        kind: EffectKind::ScreenFlash { color: Color::new(0.2, 1.0, 0.4, 0.4) },
                        lifetime: 0.5,
                        max_lifetime: 0.5,
                    });
                }
                Ok(false) => {
                    is_success = false;
                    msg = format!("[!] クホホホホ…！ 過剰精錬失敗！ +{} {} は粉々に砕け散りました…！", old_refine, eq.name);
                    self.visual_effects.push(VisualEffect {
                        from_x: 0.0,
                        from_y: 0.0,
                        to_x: 0.0,
                        to_y: 0.0,
                        kind: EffectKind::ScreenFlash { color: Color::new(1.0, 0.2, 0.2, 0.5) },
                        lifetime: 0.6,
                        max_lifetime: 0.6,
                    });
                }
                Err(err) => {
                    is_success = false;
                    msg = format!("[精錬中断] {}", err);
                }
            }
        } else {
            is_success = false;
            msg = "精錬可能な武器を装備していません。".to_string();
        }

        self.chat_logs.push(msg.clone());
        self.refine_popup = Some((msg, is_success, 3.0));
    }

    /// 安全圏(+4)までの一括精錬
    fn execute_safe_refine_to_4(&mut self) {
        if let Some(eq) = self.world.active_character.equipments.iter_mut().find(|e| e.slot == EquipSlot::Weapon) {
            let mut logs = Vec::new();
            while eq.refine < 4 && self.world.treasury_zeny >= 300 {
                self.world.treasury_zeny -= 300;
                let _ = Refinery::refine_once(eq, None, &mut logs);
            }
            let msg = format!("[安全圏精錬完了] {} は +{} になりました。(破損リスクなし)", eq.name, eq.refine);
            self.chat_logs.push(msg.clone());
            self.refine_popup = Some((msg, true, 2.5));
        }
    }

    /// 目標ワークオーダー (+7 / +10)
    fn execute_target_order(&mut self, target_lvl: u8) {
        let cost = target_lvl as u32 * 800;
        if self.world.treasury_zeny < cost {
            self.chat_logs.push(format!("[資金不足] ワークオーダー資金が足りません (必要: {} Zeny)", cost));
            return;
        }

        let order = WorkOrder {
            order_name: format!("目標+{} 特注過剰精錬", target_lvl),
            mode: WorkOrderMode::BudgetBurn {
                target_level: target_lvl,
                max_zeny: cost,
                max_weapons: 3,
            },
            target_level: target_lvl,
        };

        let smith = self.world.active_character.clone();
        let base_weapons = vec![
            Equipment::new("base1", "ツルギ", EquipSlot::Weapon, 100, 4),
            Equipment::new("base2", "ツルギ", EquipSlot::Weapon, 100, 4),
        ];

        let (report, remaining_zeny) = Refinery::process_work_order(&order, base_weapons, &smith, self.world.treasury_zeny);
        self.world.treasury_zeny = remaining_zeny;

        let has_target_weapon = report.completed_weapons.iter().any(|w| w.refine >= target_lvl);
        let msg = format!(
            "[ワークオーダー完了] 目標+{} 結果: 納品 {}本 / 破損 {}本 (消費: {} Zeny)",
            target_lvl, report.completed_weapons.len(), report.broken_count, report.total_zeny_spent
        );

        for w in report.completed_weapons {
            self.world.storage_equipments.push(w);
        }

        self.chat_logs.push(msg.clone());
        self.refine_popup = Some((msg, has_target_weapon, 3.0));
    }

    /// 選択中武器のロック切替
    fn toggle_selected_weapon_lock(&mut self) {
        if let Some(eq) = self.world.active_character.equipments.iter_mut().find(|e| e.slot == EquipSlot::Weapon) {
            eq.is_locked = !eq.is_locked;
            let status = if eq.is_locked { "ロック有効 (保護)" } else { "ロック解除" };
            self.chat_logs.push(format!("[ロック状態切替] {} を {} に設定しました。", eq.name, status));
        }
    }

    /// 選択中武器へカード装着
    fn attach_card_to_selected_weapon(&mut self) {
        if let Some(eq) = self.world.active_character.equipments.iter_mut().find(|e| e.slot == EquipSlot::Weapon) {
            if eq.cards.len() < eq.slots as usize {
                eq.cards.push(Card {
                    id: "card_skel_worker".to_string(),
                    name: "スケルワーカーカード".to_string(),
                    slot: EquipSlot::Weapon,
                    modifiers: vec![],
                });
                self.chat_logs.push(format!("[カード装着] {} の空きスロットへ 『スケルワーカーカード』 を装着しました！ (残スロット: {})", eq.name, eq.slots as usize - eq.cards.len()));
            } else {
                self.chat_logs.push("[装着不可] 空きスロットがありません。".to_string());
            }
        }
    }

    /// 遠征（自動派遣）の開始
    fn start_dispatch_expedition(&mut self, area_id: &str, area_name: &str) {
        self.active_dispatch_progress = Some((area_id.to_string(), area_name.to_string(), 0.0, 4.0));
        self.chat_logs.push(format!("[遠征開始] 控え部隊が 『{}』 へ出発しました！ (約4秒後に帰還)", area_name));
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
            self.next_growth_plan,
        );
        self.world.active_character = new_char.clone();

        self.chat_logs.push(format!(
            "[レガシー転生] 先代英雄 {} が名簿へ！ 第{}世代へ継承され、{} (Lv.1, 方針: {:?}) が加入！",
            old_hero.name, self.world.generation, new_char.name, self.next_growth_plan
        ));
    }

    /// 精錬成功/クホホ失敗ポップアップ描画
    fn draw_refine_popup(&self) {
        if let Some((ref msg, is_success, _)) = self.refine_popup {
            let cx = 415.0;
            let cy = 240.0;
            let pw = 480.0;
            let ph = 75.0;
            let px = cx - pw / 2.0;
            let py = cy - ph / 2.0;

            let (bg, border) = if is_success {
                (Color::new(0.08, 0.25, 0.15, 0.95), Color::new(0.4, 1.0, 0.6, 1.0))
            } else {
                (Color::new(0.35, 0.08, 0.10, 0.95), Color::new(1.0, 0.3, 0.4, 1.0))
            };

            draw_rectangle(px, py, pw, ph, bg);
            draw_rectangle_lines(px, py, pw, ph, 2.5, border);
            self.draw_text_helper(msg, px + 18.0, py + 42.0, 13.5, WHITE);
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
