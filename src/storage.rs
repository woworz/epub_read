use crate::model::{BookData, Mark};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};
pub fn data_dir() -> Result<PathBuf, String> {
    let dir = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("无法确定程序目录")?
        .join("data");
    fs::create_dir_all(&dir)
        .map_err(|e| format!("无法写入程序旁的 data 文件夹，请将程序移到可写目录：{e}"))?;
    Ok(dir)
}
pub fn read<T: DeserializeOwned + Default>(path: &Path) -> Result<T, String> {
    if !path.exists() {
        return Ok(T::default());
    }
    serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| format!("数据文件损坏 {}：{e}", path.display()))
}
pub fn write<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let tmp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    let mut file = File::create(&tmp).map_err(|e| e.to_string())?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    drop(file);
    // Windows 的 rename 不覆盖旧文件；MoveFileExW 原子替换，避免先删除正式文件。
    crate::native::replace_file(&tmp, path)
}
pub fn selftest(dir: &Path) -> Result<(), String> {
    let path = dir.join(format!("selftest-{}.json", std::process::id()));
    if path.exists() {
        return Err("自检临时文件已存在，拒绝覆盖".into());
    }
    let mut data = BookData::default();
    data.marks.push(Mark {
        chapter_id: "章一".into(),
        paragraph: 3,
        text_hash: "测试哈希".into(),
        text: "喜欢的中文段落".into(),
        color: 2,
        note: "往返测试".into(),
    });
    let result = (|| {
        write(&path, &data)?;
        let loaded: BookData = read(&path)?;
        if loaded.marks != data.marks || loaded.location != data.location {
            return Err("存储往返校验失败".into());
        }
        Ok(())
    })();
    let cleanup = fs::remove_file(&path).map_err(|e| e.to_string());
    result.and(cleanup)
}
