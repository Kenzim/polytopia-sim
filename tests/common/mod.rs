use std::path::PathBuf;

/// This week's challenge file, read in place: `WEEKLY_STATE`, else the first `WeeklyChallenge/5af11d91-*.state`
/// under `/home/*/.config/unity3d/Midjiwan/Polytopia/*/Data`.
pub fn weekly_file() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("WEEKLY_STATE") {
        return Some(PathBuf::from(p));
    }
    for home in std::fs::read_dir("/home").ok()?.flatten() {
        let root = home.path().join(".config/unity3d/Midjiwan/Polytopia");
        let Ok(ids) = std::fs::read_dir(&root) else { continue };
        for id in ids.flatten() {
            let Ok(files) = std::fs::read_dir(id.path().join("Data/WeeklyChallenge")) else { continue };
            for f in files.flatten() {
                let n = f.file_name().to_string_lossy().into_owned();
                if n.starts_with("5af11d91-") && n.ends_with(".state") {
                    return Some(f.path());
                }
            }
        }
    }
    None
}
