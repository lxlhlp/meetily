# uplink-updater-tauri

`@uplink/updater-sdk/tauri` 的 Rust 侧（SDK 0.6.0 起的 crate 形态，取代旧「复制
`rust/uplink-updater.rs` 模板」分发方式——模板升级无法到达宿主的问题就此根治，
升级=bump 版本号）。

## 接入

1. 宿主 `Cargo.toml`：

   ```toml
   # 推荐生产形态：git tag 依赖（升级 = 改 tag 到新版本号，与 npm SDK 版本号一致）
   uplink-updater-tauri = { git = "https://codeup.aliyun.com/jiuqian/hanzi/uplink.git", tag = "uplink-updater-tauri-v0.6.0" }
   # 本地开发（同机 uplink 仓库）可换 path 依赖
   # uplink-updater-tauri = { path = "../../uplink/packages/updater-sdk/rust/uplink-updater-tauri" }
   tauri-plugin-updater = "2"   # 本 crate 用其 UpdaterExt 能力，插件本体仍由宿主注册
   ```

   git 拉取需要 codeup 凭据（宿主团队向仓库管理员申请只读权限即可）；crate 自身
   走 git，其依赖（tauri/serde 等）仍正常从 crates.io 解析，不受影响。

2. 宿主 lib.rs（`use` 别名可让既有 `uplink_updater::` 路径零改动迁移）：

   ```rust
   use uplink_updater_tauri as uplink_updater;

   tauri::Builder::default()
       .plugin(tauri_plugin_updater::Builder::new().build())
       .plugin(uplink_updater::uplink_updater_plugin(
           base_url,        // 平台基址（生产由构建/环境注入，勿硬编码镜像——红线 2）
           "my-app".into(), // 平台应用标识
           "1.0.0".into(),  // clientUpdaterVersion
       ))
       .invoke_handler(tauri::generate_handler![
           uplink_updater::uplink_config, uplink_updater::uplink_set_channel,
           uplink_updater::uplink_check, uplink_updater::uplink_download,
           uplink_updater::uplink_install_and_relaunch,
       ])
   ```

3. `tauri.conf.json` 仍须配 `plugins.updater.pubkey`（minisign 公钥，与平台登记
   一致；endpoints 字段被运行时覆盖，占位即可）+ capabilities 增 `updater:default`。

## 从模板迁移（存量宿主）

删除 `src-tauri/src/uplink_updater.rs` 拷贝与 `mod uplink_updater;`，改 crate 依赖
+ `use uplink_updater_tauri as uplink_updater;`——命令名/插件签名与 0.5.x 模板完全
一致，前端 `@uplink/updater-sdk/tauri` 零改动。

## 验证

宿主编译即验证（本 crate 在 aidb 随应用构建/测试门禁持续编译）。独立验证：
`cargo check`（在本目录）。
