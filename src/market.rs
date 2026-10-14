use std::collections::HashMap;

/// 市場で売買されるアイテムの在庫および価格情報
#[derive(Debug, Clone)]
pub struct MarketItem {
    pub id: String,
    pub name: String,
    pub base_price: u32,
    pub current_stock: i32,
    pub target_stock: i32,
}

impl MarketItem {
    pub fn new(id: &str, name: &str, base_price: u32, initial_stock: i32, target_stock: i32) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            base_price,
            current_stock: initial_stock,
            target_stock,
        }
    }

    /// 現在の価格倍率（0.3倍 〜 3.0倍）
    pub fn price_multiplier(&self) -> f64 {
        if self.current_stock <= 0 {
            return 3.0;
        }
        let ratio = self.target_stock as f64 / self.current_stock as f64;
        ratio.clamp(0.3, 3.0)
    }

    /// 現在の市場販売価格（プレイヤー購入価格）
    pub fn buy_price(&self) -> u32 {
        let mult = self.price_multiplier();
        ((self.base_price as f64) * mult).round().max(1.0) as u32
    }

    /// 現在の市場買取価格（プレイヤー売却価格：基準は買値の60%）
    pub fn sell_price(&self, overcharge_bonus: f64) -> u32 {
        let base_sell = (self.buy_price() as f64) * 0.6;
        let final_sell = base_sell * (1.0 + overcharge_bonus);
        final_sell.round().max(1.0) as u32
    }

    /// 自然回復（目標在庫へ少しずつ復帰）
    #[allow(dead_code)]
    pub fn natural_recovery(&mut self, rate: f64) {
        let diff = self.target_stock - self.current_stock;
        if diff != 0 {
            let change = (diff as f64 * rate).round() as i32;
            if change == 0 {
                if diff > 0 {
                    self.current_stock += 1;
                } else {
                    self.current_stock -= 1;
                }
            } else {
                self.current_stock += change;
            }
        }
    }
}

/// BOM（製造レシピ）の原材料要件
#[derive(Debug, Clone)]
pub struct RecipeIngredient {
    pub item_id: String,
    pub count: u32,
}

/// BOM（製造レシピ）定義
#[derive(Debug, Clone)]
pub struct Recipe {
    pub output_item_id: String,
    pub output_count: u32,
    pub ingredients: Vec<RecipeIngredient>,
}

/// 在庫変動とBOM連動を備えた市場システム
pub struct MarketSystem {
    pub items: HashMap<String, MarketItem>,
    pub recipes: HashMap<String, Recipe>,
    pub event_log: Vec<String>,
}

impl MarketSystem {
    pub fn new() -> Self {
        let mut system = Self {
            items: HashMap::new(),
            recipes: HashMap::new(),
            event_log: Vec::new(),
        };
        system.initialize_default_market();
        system
    }

    /// デフォルトの市場アイテムとBOMレシピの初期化
    fn initialize_default_market(&mut self) {
        // 完成品・消費アイテム
        self.register_item(MarketItem::new("white_potion", "白ポーション", 120, 200, 200));
        self.register_item(MarketItem::new("damascus", "ダマスカス短剣", 35000, 20, 20));

        // 中間素材
        self.register_item(MarketItem::new("steel", "鋼鉄", 2500, 100, 100));

        // 一次素材
        self.register_item(MarketItem::new("white_herb", "白ハーブ", 30, 500, 500));
        self.register_item(MarketItem::new("empty_bottle", "空瓶", 10, 500, 500));
        self.register_item(MarketItem::new("iron_ore", "鉄鉱石", 80, 600, 600));
        self.register_item(MarketItem::new("coal", "石炭", 400, 300, 300));
        self.register_item(MarketItem::new("oridecon", "オリデオコン", 8000, 50, 50));

        // レシピ登録
        // 1. 白ポーション <= 白ハーブ x2 + 空瓶 x1
        self.register_recipe(Recipe {
            output_item_id: "white_potion".to_string(),
            output_count: 1,
            ingredients: vec![
                RecipeIngredient { item_id: "white_herb".to_string(), count: 2 },
                RecipeIngredient { item_id: "empty_bottle".to_string(), count: 1 },
            ],
        });

        // 2. 鋼鉄 <= 鉄鉱石 x2 + 石炭 x1
        self.register_recipe(Recipe {
            output_item_id: "steel".to_string(),
            output_count: 1,
            ingredients: vec![
                RecipeIngredient { item_id: "iron_ore".to_string(), count: 2 },
                RecipeIngredient { item_id: "coal".to_string(), count: 1 },
            ],
        });

        // 3. ダマスカス短剣 <= 鋼鉄 x3 + オリデオコン x1
        self.register_recipe(Recipe {
            output_item_id: "damascus".to_string(),
            output_count: 1,
            ingredients: vec![
                RecipeIngredient { item_id: "steel".to_string(), count: 3 },
                RecipeIngredient { item_id: "oridecon".to_string(), count: 1 },
            ],
        });
    }

    pub fn register_item(&mut self, item: MarketItem) {
        self.items.insert(item.id.clone(), item);
    }

    pub fn register_recipe(&mut self, recipe: Recipe) {
        self.recipes.insert(recipe.output_item_id.clone(), recipe);
    }

    /// アイテムを購入（プレイヤーが市場から仕入れ）
    /// 購入によって在庫が減少し、街の工房が在庫補充のために連鎖的に原材料を消費する！
    pub fn buy(&mut self, item_id: &str, count: u32) -> Result<u32, String> {
        let (item_name, total_cost) = {
            let item = self.items.get_mut(item_id).ok_or_else(|| format!("Item not found: {}", item_id))?;
            let unit_price = item.buy_price();
            let total = unit_price * count;
            item.current_stock -= count as i32;
            (item.name.clone(), total)
        };

        self.event_log.push(format!(
            "🛒 [購入] {} を {}個 購入 (合計 {} Zeny, 残り在庫: {})",
            item_name, count, total_cost, self.items.get(item_id).unwrap().current_stock
        ));

        // プル型需要連鎖: 工房が完成品の補充のため、原材料を連鎖消費する
        self.propagate_demand_pull(item_id, count);

        Ok(total_cost)
    }

    /// アイテムを売却（プレイヤーが市場へ卸売）
    pub fn sell(&mut self, item_id: &str, count: u32, overcharge_rate: f64) -> Result<u32, String> {
        let (item_name, total_gain) = {
            let item = self.items.get_mut(item_id).ok_or_else(|| format!("Item not found: {}", item_id))?;
            let unit_price = item.sell_price(overcharge_rate);
            let total = unit_price * count;
            item.current_stock += count as i32;
            (item.name.clone(), total)
        };

        self.event_log.push(format!(
            "💰 [売却] {} を {}個 売却 (合計 {} Zeny獲得 [OC:+{:.0}%], 現在在庫: {})",
            item_name, count, total_gain, overcharge_rate * 100.0, self.items.get(item_id).unwrap().current_stock
        ));

        Ok(total_gain)
    }

    /// プル型需要連鎖（BOMツリーの再帰的消費）
    /// 完成品が消費された際、街の鍛冶屋・調合師が原料を市場から引き出して自動補充を行う
    fn propagate_demand_pull(&mut self, item_id: &str, consumed_count: u32) {
        if let Some(recipe) = self.recipes.get(item_id).cloned() {
            // 生産に必要なバッチ数
            let batches = (consumed_count + recipe.output_count - 1) / recipe.output_count;

            for ingredient in &recipe.ingredients {
                let required_qty = ingredient.count * batches;
                if let Some(ing_item) = self.items.get_mut(&ingredient.item_id) {
                    // 原材料の在庫を引き落とし（不足しても在庫はマイナスまたは極小まで下がることで急騰を表現）
                    ing_item.current_stock -= required_qty as i32;
                    let remaining = ing_item.current_stock;
                    let ing_name = ing_item.name.clone();

                    self.event_log.push(format!(
                        "   ⚙️ [工房需要連鎖] {}の補充のため {} を {}個 消費 (在庫: {})",
                        recipe.output_item_id, ing_name, required_qty, remaining
                    ));
                }

                // 中間素材であれば、さらにその下流の原材料へ需要が波及する（再帰）
                self.propagate_demand_pull(&ingredient.item_id, required_qty);
            }
        }
    }

    /// 自然回復処理
    #[allow(dead_code)]
    pub fn tick_recovery(&mut self, rate: f64) {
        for item in self.items.values_mut() {
            item.natural_recovery(rate);
        }
    }

    /// 特需イベント（特定アイテムの需要急増など）
    #[allow(dead_code)]
    pub fn trigger_special_demand(&mut self, item_id: &str, drain_amount: i32, reason: &str) {
        if let Some(item) = self.items.get_mut(item_id) {
            item.current_stock -= drain_amount;
            self.event_log.push(format!(
                "📢 [特需イベント発令] 『{}』により、{} の在庫が {}個 激減！(現在在庫: {})",
                reason, item.name, drain_amount, item.current_stock
            ));
        }
        self.propagate_demand_pull(item_id, drain_amount.max(0) as u32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_market_dynamic_pricing_and_bom_pull() {
        let mut market = MarketSystem::new();

        let initial_pot_price = market.items.get("white_potion").unwrap().buy_price();
        let initial_herb_price = market.items.get("white_herb").unwrap().sell_price(0.0);

        // 白ポーションを100個購入
        let cost = market.buy("white_potion", 100).expect("Buy failed");
        assert!(cost > 0);

        let after_pot = market.items.get("white_potion").unwrap();
        assert_eq!(after_pot.current_stock, 100);
        // 在庫半減により買値が高騰
        assert!(after_pot.buy_price() > initial_pot_price);

        // 工房が白ハーブを200個連鎖消費
        let after_herb = market.items.get("white_herb").unwrap();
        assert_eq!(after_herb.current_stock, 300);
        assert!(after_herb.sell_price(0.0) > initial_herb_price);
    }
}
