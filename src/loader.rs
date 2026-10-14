use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::models::{Card, Job, MonsterTemplate, Skill};

/// mods/core/ 等のMODデータ群を一元管理するデータベース
#[derive(Debug, Clone, Default)]
pub struct ModDatabase {
    pub jobs: HashMap<String, Job>,
    pub cards: HashMap<String, Card>,
    pub skills: HashMap<String, Skill>,
    pub monsters: HashMap<String, MonsterTemplate>,
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

        Ok(db)
    }
}
