fn main() {
    tauri_build::build();
    embed_test_manifest();
}

/// 为集成测试目标补嵌 Windows 应用程序清单。
///
/// 背景：`tests/*.rs` 里跑 tauri mock 运行时（`tauri::test`）时，构造
/// `WebviewWindow` 会把 tao 拖进来，而 tao 导入了 comctl32 **v6** 才有的
/// `TaskDialogIndirect`。该符号要在加载时解析到 v6，前提是进程内嵌一份声明
/// `Microsoft.Windows.Common-Controls 6.0` 依赖的清单；否则加载器绑定
/// `System32\comctl32.dll`（v5.82，不导出 `TaskDialogIndirect`），进程启动即
/// `STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139)`。
///
/// tauri-build 只通过 `cargo:rustc-link-arg-bins` 把清单资源发给 **bin** 目标，
/// 测试目标拿不到，故在此按同样内容单独发给 tests 目标。非 Windows 平台为 no-op。
fn embed_test_manifest() {
    if !cfg!(windows) {
        return;
    }

    const TEST_MANIFEST: &str = r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>
</assembly>
"#;

    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR 未设置"));
    let manifest_path = out_dir.join("test-app-manifest.xml");
    std::fs::write(&manifest_path, TEST_MANIFEST).expect("写入测试清单失败");

    println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-tests=/MANIFESTINPUT:{}",
        manifest_path.display()
    );
    println!("cargo:rustc-link-arg-tests=/MANIFESTUAC:NO");
}
