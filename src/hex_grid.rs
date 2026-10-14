use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

/// Pointy-Topped Axial ヘクス座標 (q, r)
/// Cube座標 (x, y, z) との関係:
/// x = q, z = r, y = -q - r (x + y + z = 0)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HexCoord {
    pub q: i32,
    pub r: i32,
}

impl HexCoord {
    pub const fn new(q: i32, r: i32) -> Self {
        Self { q, r }
    }

    /// Cube座標の3番目の軸 s (y) を算出
    pub const fn s(&self) -> i32 {
        -self.q - self.r
    }

    /// 2つのヘクス間のCube距離（Manhattan距離）を計算
    pub fn distance(&self, other: HexCoord) -> u32 {
        let dq = (self.q - other.q).abs();
        let dr = (self.r - other.r).abs();
        let ds = (self.s() - other.s()).abs();
        ((dq + dr + ds) / 2) as u32
    }

    /// 6方向の隣接ヘクス差分（Pointy-topped）
    /// 東(+1, 0), 北東(+1, -1), 北西(0, -1), 西(-1, 0), 南西(-1, +1), 南東(0, +1)
    pub const DIRECTIONS: [HexCoord; 6] = [
        HexCoord::new(1, 0),   // East
        HexCoord::new(1, -1),  // NorthEast
        HexCoord::new(0, -1),  // NorthWest
        HexCoord::new(-1, 0),  // West
        HexCoord::new(-1, 1),  // SouthWest
        HexCoord::new(0, 1),   // SouthEast
    ];

    /// 指定方向へ1ヘクス移動
    pub fn neighbor(&self, direction_idx: usize) -> HexCoord {
        let dir = Self::DIRECTIONS[direction_idx % 6];
        HexCoord::new(self.q + dir.q, self.r + dir.r)
    }

    /// 6つの全隣接ヘクスを取得
    pub fn neighbors(&self) -> [HexCoord; 6] {
        [
            self.neighbor(0),
            self.neighbor(1),
            self.neighbor(2),
            self.neighbor(3),
            self.neighbor(4),
            self.neighbor(5),
        ]
    }

    /// 自身から target への大まかなヘクス方向インデックス（0..6）を計算
    pub fn direction_to(&self, target: HexCoord) -> usize {
        let dq = target.q - self.q;
        let dr = target.r - self.r;
        let ds = target.s() - self.s();

        // 差分が0の場合はデフォルト0
        if dq == 0 && dr == 0 {
            return 0;
        }

        let mut best_dir = 0;
        let mut best_dot = i32::MIN;

        for (i, dir) in Self::DIRECTIONS.iter().enumerate() {
            // 内積 (dq*dir.q + dr*dir.r + ds*dir.s)
            let dot = dq * dir.q + dr * dir.r + ds * dir.s();
            if dot > best_dot {
                best_dot = dot;
                best_dir = i;
            }
        }
        best_dir
    }

    /// 自身から相手を見て「後方（ノックバック方向）」へ指定距離離れたヘクスを算出
    pub fn knockback_target(&self, from: HexCoord, distance: u32) -> HexCoord {
        if *self == from {
            // 同一位置の場合はデフォルト東方向
            return self.neighbor(0);
        }
        let dir_idx = from.direction_to(*self);
        let dir = Self::DIRECTIONS[dir_idx];
        HexCoord::new(
            self.q + dir.q * distance as i32,
            self.r + dir.r * distance as i32,
        )
    }

    /// 指定の中心ヘクスから半径 radius 内の全ヘクス座標リストを取得（AoE範囲判定用）
    pub fn get_hexes_in_radius(center: HexCoord, radius: u32) -> Vec<HexCoord> {
        let mut results = Vec::new();
        let r = radius as i32;
        for q in -r..=r {
            let r_min = (-r).max(-q - r);
            let r_max = r.min(-q + r);
            for r_val in r_min..=r_max {
                results.push(HexCoord::new(center.q + q, center.r + r_val));
            }
        }
        results
    }

    /// クォータービュー（アイソメトリック）2Dスクリーン座標への変換
    /// Pointy-toppedヘクスを横長・縦圧縮（2:1アイソメ比率）に投影
    pub fn hex_to_pixel(coord: HexCoord, hex_size: f32) -> (f32, f32) {
        let sqrt3 = 3.0_f32.sqrt();
        let x = hex_size * (sqrt3 * (coord.q as f32) + (sqrt3 / 2.0) * (coord.r as f32));
        // Y軸を約0.58倍に潰すことで、古典クォータービュー（2.5D）の菱形立体感を再現
        let iso_y_ratio = 0.58;
        let y = hex_size * (1.5 * (coord.r as f32)) * iso_y_ratio;
        (x, y)
    }

    /// 2Dスクリーン座標から最も近いHexCoordへの逆変換
    pub fn pixel_to_hex(x: f32, y: f32, hex_size: f32) -> HexCoord {
        let iso_y_ratio = 0.58;
        let uncompressed_y = y / iso_y_ratio;
        let sqrt3 = 3.0_f32.sqrt();

        let q_frac = (sqrt3 / 3.0 * x - 1.0 / 3.0 * uncompressed_y) / hex_size;
        let r_frac = (2.0 / 3.0 * uncompressed_y) / hex_size;
        let s_frac = -q_frac - r_frac;

        let mut q = q_frac.round() as i32;
        let mut r = r_frac.round() as i32;
        let s = s_frac.round() as i32;

        let q_diff = (q as f32 - q_frac).abs();
        let r_diff = (r as f32 - r_frac).abs();
        let s_diff = (s as f32 - s_frac).abs();

        if q_diff > r_diff && q_diff > s_diff {
            q = -r - s;
        } else if r_diff > s_diff {
            r = -q - s;
        }
        HexCoord::new(q, r)
    }
}

/// 建物・障害物の種類
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObstacleType {
    Wall,     // 城壁・防壁
    Building, // 四角い石造りの建物
    Rock,     // 巨大な岩
    Pillar,   // 柱
}

/// 四角い建物・障害物のヘクス占有フットプリント
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildingFootprint {
    pub id: String,
    pub name: String,
    pub obstacle_type: ObstacleType,
    pub origin: HexCoord,
    pub width: u32,
    pub height: u32,
}

impl BuildingFootprint {
    pub fn new(
        id: &str,
        name: &str,
        obstacle_type: ObstacleType,
        origin: HexCoord,
        width: u32,
        height: u32,
    ) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            obstacle_type,
            origin,
            width,
            height,
        }
    }

    /// この建物が占有する全ヘクス座標（通行不可領域）を取得
    pub fn get_occupied_hexes(&self) -> Vec<HexCoord> {
        let mut hexes = Vec::new();
        for dq in 0..self.width {
            for dr in 0..self.height {
                hexes.push(HexCoord::new(
                    self.origin.q + dq as i32,
                    self.origin.r + dr as i32,
                ));
            }
        }
        hexes
    }
}

/// ヘクスグリッド戦場マップ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HexMap {
    pub id: String,
    pub name: String,
    pub min_q: i32,
    pub max_q: i32,
    pub min_r: i32,
    pub max_r: i32,
    pub blocked_hexes: HashSet<HexCoord>,
    pub buildings: Vec<BuildingFootprint>,
}

impl HexMap {
    pub fn new(
        id: &str,
        name: &str,
        min_q: i32,
        max_q: i32,
        min_r: i32,
        max_r: i32,
    ) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            min_q,
            max_q,
            min_r,
            max_r,
            blocked_hexes: HashSet::new(),
            buildings: Vec::new(),
        }
    }

    /// 四角い建物を配置し、フットプリントを自動で通行不可マスクに追加
    pub fn add_building(&mut self, building: BuildingFootprint) {
        for hex in building.get_occupied_hexes() {
            self.blocked_hexes.insert(hex);
        }
        self.buildings.push(building);
    }

    /// 座標がマップ境界内にあるか判定
    pub fn is_within_bounds(&self, coord: HexCoord) -> bool {
        coord.q >= self.min_q
            && coord.q <= self.max_q
            && coord.r >= self.min_r
            && coord.r <= self.max_r
    }

    /// 座標が通行可能か判定（マップ範囲内かつ障害物なし）
    pub fn is_walkable(&self, coord: HexCoord) -> bool {
        self.is_within_bounds(coord) && !self.blocked_hexes.contains(&coord)
    }

    /// 検証シナリオ用：中央に狭い通路（チョークポイント: 幅1ヘクス）を持つ防衛マップを構築
    /// 左側に後衛・前衛、右側にモンスター群が配置され、中央の狭路で激突する
    pub fn create_chokepoint_map() -> Self {
        let mut map = HexMap::new("chokepoint_arena", "監視塔の隘路（チョークポイント）", -2, 10, -2, 8);

        // 上部を塞ぐ大型城壁 (origin: q:4, r:-2, width: 2, height: 4) -> (4..5, -2..1)
        let north_wall = BuildingFootprint::new(
            "north_bastion",
            "北側監視城壁",
            ObstacleType::Wall,
            HexCoord::new(4, -2),
            2,
            4,
        );
        map.add_building(north_wall);

        // 下部を塞ぐ兵舎施設 (origin: q:4, r:3, width: 2, height: 5) -> (4..5, 3..7)
        // これにより q=4..5, r=2 付近に幅1ヘクスの隘路（チョークポイント）が形成される
        let south_barracks = BuildingFootprint::new(
            "south_barracks",
            "南側防衛兵舎",
            ObstacleType::Building,
            HexCoord::new(4, 3),
            2,
            5,
        );
        map.add_building(south_barracks);

        map
    }

    /// GvG用：左右対称の柱や石碑が存在するアリーナマップ
    pub fn create_gvg_arena_map() -> Self {
        let mut map = HexMap::new("gvg_hall", "栄光の騎士団闘技場", -4, 12, -3, 9);

        // 中央左右に配置された巨大な障害物柱
        let pillar_top = BuildingFootprint::new(
            "pillar_top",
            "試練の大石柱(北)",
            ObstacleType::Pillar,
            HexCoord::new(3, 0),
            2,
            2,
        );
        let pillar_bottom = BuildingFootprint::new(
            "pillar_bottom",
            "試練の大石柱(南)",
            ObstacleType::Pillar,
            HexCoord::new(4, 5),
            2,
            2,
        );
        map.add_building(pillar_top);
        map.add_building(pillar_bottom);

        map
    }

    /// 迎撃戦用：入り組んだ岩石と鉱山支柱が立ち並ぶ『廃坑深層』マップ
    pub fn create_mine_depths_map() -> Self {
        let mut map = HexMap::new("mine_depths", "廃坑深層（地下坑道）", -3, 11, -3, 8);

        // 坑道の巨大岩石群
        let boulder_1 = BuildingFootprint::new(
            "boulder_1",
            "崩落した巨岩",
            ObstacleType::Rock,
            HexCoord::new(3, -1),
            2,
            2,
        );
        let boulder_2 = BuildingFootprint::new(
            "boulder_2",
            "結晶鉱石の塊",
            ObstacleType::Rock,
            HexCoord::new(5, 4),
            2,
            2,
        );
        let support_beam = BuildingFootprint::new(
            "support_beam",
            "坑道支柱",
            ObstacleType::Pillar,
            HexCoord::new(1, 2),
            1,
            2,
        );

        map.add_building(boulder_1);
        map.add_building(boulder_2);
        map.add_building(support_beam);

        map
    }
}

/// A* 経路探索用の優先度付きキューノード
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AStarNode {
    coord: HexCoord,
    cost: u32,       // g(n): スタートからの実移動コスト
    priority: u32,   // f(n) = g(n) + h(n)
}

impl Ord for AStarNode {
    fn cmp(&self, other: &Self) -> Ordering {
        // 最小ヒープとして使うため、priorityが小さい方を優先（逆順）
        other.priority.cmp(&self.priority)
    }
}

impl PartialOrd for AStarNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A* 経路探索実装
pub struct HexPathfinder;

impl HexPathfinder {
    /// 通常のA*探索：start から goal までの完全経路を算出
    /// `occupied_hexes` には他ユニットが存在するヘクスを指定（通行不可扱い）
    pub fn find_path(
        map: &HexMap,
        start: HexCoord,
        goal: HexCoord,
        occupied_hexes: &HashSet<HexCoord>,
    ) -> Option<Vec<HexCoord>> {
        Self::find_path_to_range(map, start, goal, 0, occupied_hexes)
    }

    /// 空間タクティクス用 A* 探索：
    /// ターゲット `target` の射程 `attack_range` 以内のヘクスに到達する最短経路を算出。
    /// もし既に射程内にいる場合は空の Vec (移動不要) を返す。
    pub fn find_path_to_range(
        map: &HexMap,
        start: HexCoord,
        target: HexCoord,
        attack_range: u32,
        occupied_hexes: &HashSet<HexCoord>,
    ) -> Option<Vec<HexCoord>> {
        // すでに射程内にいる場合、移動の必要なし
        if start.distance(target) <= attack_range {
            return Some(Vec::new());
        }

        let mut open_set = BinaryHeap::new();
        let mut came_from: HashMap<HexCoord, HexCoord> = HashMap::new();
        let mut g_score: HashMap<HexCoord, u32> = HashMap::new();

        g_score.insert(start, 0);

        let initial_h = start.distance(target).saturating_sub(attack_range);
        open_set.push(AStarNode {
            coord: start,
            cost: 0,
            priority: initial_h,
        });

        let mut closest_node_in_range: Option<HexCoord> = None;

        while let Some(current_node) = open_set.pop() {
            let current = current_node.coord;

            // ゴール判定：ターゲットとの距離が attack_range 以下となった場合
            if current.distance(target) <= attack_range {
                closest_node_in_range = Some(current);
                break;
            }

            let current_g = match g_score.get(&current) {
                Some(&g) => g,
                None => continue,
            };

            for neighbor in current.neighbors() {
                // マップ外・建物障害物は通行不可
                if !map.is_walkable(neighbor) {
                    continue;
                }
                // 他のユニットが存在するヘクスは通行不可（ただしスタート位置や探索中自身は除く）
                if occupied_hexes.contains(&neighbor) && neighbor != target {
                    continue;
                }

                let tentative_g = current_g + 1;
                let existing_g = g_score.get(&neighbor).copied().unwrap_or(u32::MAX);

                if tentative_g < existing_g {
                    came_from.insert(neighbor, current);
                    g_score.insert(neighbor, tentative_g);

                    let h = neighbor.distance(target).saturating_sub(attack_range);
                    open_set.push(AStarNode {
                        coord: neighbor,
                        cost: tentative_g,
                        priority: tentative_g + h,
                    });
                }
            }
        }

        // ゴール到達ヘクスからスタートまでの経路を再構築
        if let Some(mut curr) = closest_node_in_range {
            let mut path = Vec::new();
            while curr != start {
                path.push(curr);
                if let Some(&prev) = came_from.get(&curr) {
                    curr = prev;
                } else {
                    break;
                }
            }
            path.reverse();
            Some(path)
        } else {
            None // 経路が見つからなかった（完全に塞がれている）
        }
    }
}

/// CUIコンソール表示用のユニット情報
#[derive(Debug, Clone)]
pub struct AsciiUnitDisplay {
    pub coord: HexCoord,
    pub symbol: String, // 例: "P1", "P2", "M1", "M2" または絵文字
    #[allow(dead_code)]
    pub is_team_a: bool,
    pub current_hp: i32,
    pub max_hp: u32,
}

/// CUIコンソール用のアスキー・ヘクス戦場マップ描画関数
pub fn render_ascii_map(
    map: &HexMap,
    units: &[AsciiUnitDisplay],
    highlight_hexes: Option<&HashSet<HexCoord>>, // AoE範囲などのハイライト
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "┌────────────────────────────────────────────────────────┐\n"
    ));
    out.push_str(&format!(
        "│ 🗺️  戦場マップ: {:<38} │\n",
        map.name
    ));
    out.push_str(&format!(
        "└────────────────────────────────────────────────────────┘\n"
    ));

    // 各ヘクスに存在するユニットの逆引き辞書
    let mut unit_map: HashMap<HexCoord, &AsciiUnitDisplay> = HashMap::new();
    for u in units {
        unit_map.insert(u.coord, u);
    }

    for r in map.min_r..=map.max_r {
        // 奇数行インデントによるPointy-toppedヘクスのジグザグ配置表現
        let indent = if r % 2 != 0 { "  " } else { "" };
        out.push_str(indent);
        out.push_str(&format!("{:>2} ", r));

        for q in map.min_q..=map.max_q {
            let coord = HexCoord::new(q, r);

            if let Some(unit) = unit_map.get(&coord) {
                // ユニット表示
                out.push_str(&format!("[{}]", unit.symbol));
            } else if map.blocked_hexes.contains(&coord) {
                // 建物・障害物
                out.push_str("[##]");
            } else if highlight_hexes.map_or(false, |hs| hs.contains(&coord)) {
                // AoEハイライト等の範囲
                out.push_str("[* ]");
            } else {
                // 通常の通行可能空きヘクス
                out.push_str(" .  ");
            }
        }
        out.push('\n');
    }

    out.push_str("凡例: [##] 建物/防壁  . 空き地  [*] 範囲攻撃着弾点  [P*] 味方  [E*]/[B*] 敵対ユニット\n");
    if !units.is_empty() {
        out.push_str("状態: ");
        let unit_statuses = units
            .iter()
            .map(|u| format!("{}: HP {}/{}", u.symbol, u.current_hp.max(0), u.max_hp))
            .collect::<Vec<_>>()
            .join(" | ");
        out.push_str(&unit_statuses);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_distance_and_neighbors() {
        let origin = HexCoord::new(0, 0);
        let p1 = HexCoord::new(2, -1); // Cube: (2, -1, -1) -> dist: 2
        assert_eq!(origin.distance(p1), 2);

        let p2 = HexCoord::new(-3, 1); // Cube: (-3, 1, 2) -> dist: 3
        assert_eq!(origin.distance(p2), 3);

        let neighbors = origin.neighbors();
        assert_eq!(neighbors.len(), 6);
        for n in &neighbors {
            assert_eq!(origin.distance(*n), 1);
        }
    }

    #[test]
    fn test_hex_screen_pixel_conversion() {
        let coord = HexCoord::new(3, 4);
        let hex_size = 20.0;
        let (x, y) = HexCoord::hex_to_pixel(coord, hex_size);
        let recovered = HexCoord::pixel_to_hex(x, y, hex_size);
        assert_eq!(coord, recovered);
    }

    #[test]
    fn test_building_footprint_blocks_hexes() {
        let mut map = HexMap::new("test_map", "テストマップ", -5, 5, -5, 5);
        let building = BuildingFootprint::new(
            "rock",
            "大岩",
            ObstacleType::Rock,
            HexCoord::new(1, 1),
            2,
            2,
        );
        map.add_building(building);

        assert!(map.blocked_hexes.contains(&HexCoord::new(1, 1)));
        assert!(map.blocked_hexes.contains(&HexCoord::new(2, 1)));
        assert!(map.blocked_hexes.contains(&HexCoord::new(1, 2)));
        assert!(map.blocked_hexes.contains(&HexCoord::new(2, 2)));
        assert!(!map.is_walkable(HexCoord::new(1, 1)));
        assert!(map.is_walkable(HexCoord::new(0, 0)));
    }

    #[test]
    fn test_astar_pathfinding_around_building() {
        let mut map = HexMap::new("test_map", "テストマップ", -5, 5, -5, 5);
        // (0, -1) から (0, 2) の縦方向を塞ぐ防壁
        let wall = BuildingFootprint::new(
            "wall",
            "壁",
            ObstacleType::Wall,
            HexCoord::new(0, -1),
            1,
            4,
        );
        map.add_building(wall);

        let start = HexCoord::new(-1, 0);
        let goal = HexCoord::new(1, 0);
        let occupied = HashSet::new();

        let path = HexPathfinder::find_path(&map, start, goal, &occupied);
        assert!(path.is_some());
        let path = path.unwrap();
        assert!(!path.is_empty());
        assert_eq!(path.last(), Some(&goal));
        // 壁のヘクスを通っていないことを検証
        for step in &path {
            assert!(!map.blocked_hexes.contains(step));
        }
    }

    #[test]
    fn test_astar_pathfinding_with_attack_range() {
        let map = HexMap::new("open_field", "広場", -5, 5, -5, 5);
        let start = HexCoord::new(-4, 0);
        let target = HexCoord::new(4, 0); // 距離 8
        let occupied = HashSet::new();

        // 弓・魔法 (射程 4) の場合、ターゲットから距離4の地点で停止
        let path = HexPathfinder::find_path_to_range(&map, start, target, 4, &occupied);
        assert!(path.is_some());
        let path = path.unwrap();
        let end_point = path.last().copied().unwrap_or(start);
        assert!(end_point.distance(target) <= 4);
        assert!(path.len() <= 5);
    }
}
