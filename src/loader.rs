use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::models::{AreaDef, Card, ItemDef, ItemsAndRecipesData, Job, MonsterTemplate, RecipeDef, Skill};

/// mods/core/ 等のMODデータ群を一元管理するデータベース
#[derive(Debug, Clone, Default)]
pub struct ModDatabase {
    pub jobs: HashMap<String, Job>,
    pub cards: HashMap<String, Card>,
    pub skills: HashMap<String, Skill>,
    pub monsters: HashMap<String, MonsterTemplate>,
    pub items: HashMap<String, ItemDef>,
    pub recipes: HashMap<String, RecipeDef>,
    pub areas: HashMap<String, AreaDef>,
}

impl ModDatabase {
    /// 指定されたMODディレクトリ（通常 "mods/core"）からすべての定義ファイルを読み込む
    pub fn load_from_dir<P: AsRef<Path>>(mod_dir: P) -> Result<Self, Box<dyn std::error::Error>> {
        let dir = mod_dir.as_ref();
        let mut db = ModDatabase::default();

        // 1. 職業 (jobs.ron)
        let jobs_path = dir.join("jobs.ron");
        if jobs_path.exists() {
            let content = fs::read_to_string(&jobs_path)
                .map_err(|e| format!("{} の読み込みに失敗しました: {}", jobs_path.display(), e))?;
            let list: Vec<Job> = ron::from_str(&content)
                .map_err(|e| format!("{} のパースに失敗しました: {}", jobs_path.display(), e))?;
            for item in list {
                db.jobs.insert(item.id.clone(), item);
            }
        }

        // 2. カード (cards.ron)
        let cards_path = dir.join("cards.ron");
        if cards_path.exists() {
            let content = fs::read_to_string(&cards_path)
                .map_err(|e| format!("{} の読み込みに失敗しました: {}", cards_path.display(), e))?;
            let list: Vec<Card> = ron::from_str(&content)
                .map_err(|e| format!("{} のパースに失敗しました: {}", cards_path.display(), e))?;
            for item in list {
                db.cards.insert(item.id.clone(), item);
            }
        }

        // 3. スキル (skills.ron)
        let skills_path = dir.join("skills.ron");
        if skills_path.exists() {
            let content = fs::read_to_string(&skills_path)
                .map_err(|e| format!("{} の読み込みに失敗しました: {}", skills_path.display(), e))?;
            let list: Vec<Skill> = ron::from_str(&content)
                .map_err(|e| format!("{} のパースに失敗しました: {}", skills_path.display(), e))?;
            for item in list {
                db.skills.insert(item.id.clone(), item);
            }
        }

        // 4. モンスター (monsters.ron)
        let monsters_path = dir.join("monsters.ron");
        if monsters_path.exists() {
            let content = fs::read_to_string(&monsters_path)
                .map_err(|e| format!("{} の読み込みに失敗しました: {}", monsters_path.display(), e))?;
            let list: Vec<MonsterTemplate> = ron::from_str(&content)
                .map_err(|e| format!("{} のパースに失敗しました: {}", monsters_path.display(), e))?;
            for item in list {
                db.monsters.insert(item.id.clone(), item);
            }
        }

        // 5. アイテムとレシピ (items_recipes.ron)
        let items_path = dir.join("items_recipes.ron");
        if items_path.exists() {
            let content = fs::read_to_string(&items_path)
                .map_err(|e| format!("{} の読み込みに失敗しました: {}", items_path.display(), e))?;
            let data: ItemsAndRecipesData = ron::from_str(&content)
                .map_err(|e| format!("{} のパースに失敗しました: {}", items_path.display(), e))?;
            for item in data.items {
                db.items.insert(item.id.clone(), item);
            }
            for recipe in data.recipes {
                db.recipes.insert(recipe.output_item_id.clone(), recipe);
            }
        }

        // 6. 狩場エリア (areas.ron)
        let areas_path = dir.join("areas.ron");
        if areas_path.exists() {
            let content = fs::read_to_string(&areas_path)
                .map_err(|e| format!("{} の読み込みに失敗しました: {}", areas_path.display(), e))?;
            let list: Vec<AreaDef> = ron::from_str(&content)
                .map_err(|e| format!("{} のパースに失敗しました: {}", areas_path.display(), e))?;
            for area in list {
                db.areas.insert(area.id.clone(), area);
            }
        }

        Ok(db)
    }
}
