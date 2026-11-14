use crate::model::Config;
use std::path::{Path, PathBuf};

pub fn path(config_path: &Path, id: u32) -> PathBuf {
    config_path.with_file_name(format!("profile-{id}.json"))
}

pub fn discover(config_path: &Path) -> Result<Vec<u32>, String> {
    // Keep legacy profiles recoverable, and never overwrite a newer preset.
    for (id, legacy) in [(1, "mode2"), (2, "mode4")] {
        let old = config_path.with_file_name(format!("profile-{legacy}.json"));
        let new = path(config_path, id);
        if old.exists() && !new.exists() {
            Config::load(&old)?.save(&new)?;
        }
    }
    let mut ids = vec![1, 2];
    let directory = config_path.parent().ok_or("配置目录不存在")?;
    for entry in std::fs::read_dir(directory).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
            continue;
        }
        let name = entry.file_name();
        if let Some(id) = name
            .to_str()
            .and_then(|s| s.strip_prefix("profile-"))
            .and_then(|s| s.strip_suffix(".json"))
            .and_then(|s| s.parse::<u32>().ok())
            .filter(|id| *id > 0)
        {
            ids.push(id);
        }
    }
    ids.sort_unstable();
    ids.dedup();
    Ok(ids)
}

pub fn add(config_path: &Path, ids: &mut Vec<u32>, config: &Config) -> Result<u32, String> {
    let id = ids
        .iter()
        .copied()
        .max()
        .unwrap_or(2)
        .checked_add(1)
        .ok_or("预设编号已用尽")?;
    if path(config_path, id).exists() {
        return Err("该预设已存在，请重启以刷新预设列表".into());
    }
    config.save(&path(config_path, id))?;
    ids.push(id);
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_and_add_presets_survive_restart_without_overwriting() {
        let dir = std::env::temp_dir().join(format!("roundkeys-presets-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let config_path = dir.join("config.json");
        let mut old = Config::default();
        old.set_layout(5, vec![]);
        old.save(&dir.join("profile-mode2.json")).unwrap();
        let mut ids = discover(&config_path).unwrap();
        assert_eq!(ids, vec![1, 2]);
        assert_eq!(Config::load(&path(&config_path, 1)).unwrap().key_count, 5);
        let mut updated = Config::default();
        updated.set_layout(14, vec![]);
        updated.save(&path(&config_path, 1)).unwrap();
        assert_eq!(add(&config_path, &mut ids, &updated).unwrap(), 3);
        assert_eq!(discover(&config_path).unwrap(), vec![1, 2, 3]);
        assert_eq!(Config::load(&path(&config_path, 1)).unwrap().key_count, 14);
        assert_eq!(Config::load(&path(&config_path, 3)).unwrap().key_count, 14);
        assert_eq!(
            Config::load(&dir.join("profile-mode2.json"))
                .unwrap()
                .key_count,
            5
        );
        // Only the explicit files created by this test are removed.
        for name in [
            "profile-mode2.json",
            "profile-1.json",
            "profile-1.json.bak",
            "profile-3.json",
        ] {
            std::fs::remove_file(dir.join(name)).unwrap();
        }
        std::fs::remove_dir(dir).unwrap();
    }
}
