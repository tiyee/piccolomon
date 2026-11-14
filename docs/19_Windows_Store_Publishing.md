# Windows 商店发布

`.github/workflows/windows-store.yml` 通过手动 **Run workflow** 构建 x64 MSIX，
可选使用微软官方 `microsoft-store-apppublisher` / `msstore` 提交商店更新。
普通推送、PR 和多平台打包不会提交商店。

## 手动生成三架构商店包

在 Actions 选择 **Build unsigned MSIX bundle for Store → Run workflow**，填入四段
`version` 和 Partner Center **Product identity** 中的 `identity_name`、`publisher`、
`publisher_display_name`。工作流并行构建 x86、x64（AMD64）和 ARM64 的原生 MSIX，
共用同一包身份和版本。三份子包齐全后，`MakeAppx` 生成并检查 `.msixbundle`。

下载 `Ryne-<version>-windows-unsigned-msixbundle` Artifact，取出 `Ryne-<version>.msixbundle`
上传到 Partner Center，随附的 `.sha256` 可用于校验下载。商店负责分发签名，Windows
安装时会选择适合设备架构的子包。

## 首次准备

1. 在 Partner Center 注册开发者账户、预留应用名称，创建 **MSIX** 类型的产品。
   使用 **Product identity** 中的真实标识，不自行编造包名或发布者。
2. 在 Partner Center 完成首次发布，包括商店介绍、截图、隐私政策、年龄分级、
   市场及定价。此工作流用于之后的包更新，不创建产品或补齐商店资料。
   首次可将上面生成的 `.msixbundle` 手动上传 Partner Center。
3. 为 Partner Center 关联 Microsoft Entra 租户，注册应用并创建客户端密钥，
   将该应用加入 Partner Center 并授予管理目标应用提交的权限。
4. GitHub 仓库 **Settings → Secrets and variables → Actions → Variables** 配置：

   | Variable | 值来源 |
   | --- | --- |
   | `STORE_IDENTITY_NAME` | Product identity → Package/Identity/Name |
   | `STORE_PUBLISHER` | Product identity → Package/Identity/Publisher，完整 `CN=…` 值 |
   | `STORE_PUBLISHER_DISPLAY_NAME` | Product identity → Package/Properties/PublisherDisplayName |
   | `STORE_PRODUCT_ID` | Store ID / Product ID，例如 `9…`；不是 Identity Name |

   前三个必须是仓库级 Variables，打包任务不使用发布环境。
   `STORE_PRODUCT_ID` 也可配置在 `microsoft-store` Environment。

5. 在仓库的 **Secrets** 或 **Environments → microsoft-store → Secrets** 配置：

   | Secret | 内容 |
   | --- | --- |
   | `AZURE_AD_TENANT_ID` | Microsoft Entra 租户 ID |
   | `AZURE_AD_APPLICATION_CLIENT_ID` | 应用 / 客户端 ID |
   | `AZURE_AD_APPLICATION_SECRET` | 客户端密钥值，而非密钥 ID |
   | `SELLER_ID` | Partner Center Seller ID |

   这些 Secrets 仅注入发布任务，生成 MSIX 无需凭据。
   可按团队需要为 `microsoft-store` 配置允许发布的分支和审核人员。

## 运行

在 Actions 选择 **Publish to Microsoft Store → Run workflow**，选择要发布的代码 ref：

- `version`：四段版本，例如 `1.0.1.0`。每段为 0–65535，末段必须为 0，
  并高于商店已发布的包版本。它独立于 Cargo 的三段应用版本。
- `submit` 默认关闭：只构建并上传 `Ryne-windows-store` Artifact，包含 MSIX、
  SHA-256 和实际 `AppxManifest.xml`，保留 14 天。
- `submit` 开启：构建成功后，发布任务下载同一份 Artifact，校验 SHA-256，
  认证 Partner Center，执行 `msstore publish`，随后轮询提交结果。
  审核可能超过任务的 60 分钟期限；超时后以 Partner Center 状态为准，
  不应立即重提相同版本。

微软 CLI 的 `publish` 会基于上次已发布提交建立更新，可能替换尚未提交的草稿。
运行前应完成或备份 Partner Center 中正在编辑的草稿。版本与应用身份不匹配、
凭据过期、权限不足、商店审核失败等会使工作流失败；成功打包不代表通过商店认证。
当前官方 GitHub Actions 发布指南将自动更新范围限定为免费产品，付费产品应先核实
当前 CLI / Partner Center 支持情况。

## 包内容与本地构建

MSIX 复用 `scripts/bundle-windows.ps1` 的发布构建、静态 MSVC 运行库、README
及原始 STIX 字体许可。`examples/store-assets.rs` 从 `assets/logo.svg` 生成
44×44、150×150 和 50×50 PNG，`packaging/windows/AppxManifest.xml` 声明七种语言、
桌面设备族和原生 Win32 应用所需的 `runFullTrust`。在 Partner Center 提供该受限
能力的用途说明：直接编辑和保存用户选择的本地 Markdown 文件。
包的最低系统声明为 Windows 10 2004（10.0.19041.0）；仍需实际验证安装、启动、
打开/保存和 IME 行为，SDK 打包检查不能代替这些验收。

在安装了 Rust、Visual Studio C++ Build Tools、Windows SDK 和 PowerShell 7 的
Windows x64 主机运行（示例身份值须替换）：

```powershell
pwsh -File scripts/bundle-windows-store.ps1 `
  -Version 1.0.1.0 `
  -IdentityName 'YourPartnerCenter.IdentityName' `
  -Publisher 'CN=Your-Partner-Center-Publisher' `
  -PublisherDisplayName 'Your Publisher Name'
```

默认产物为 `dist/store/Ryne-1.0.1.0-x64.msix`。可通过 `-Architecture x86` 或
`-Architecture arm64` 生成其他架构的单包，需安装相应的 Rust MSVC 目标、Visual Studio
C++ 工具集，ARM64 还需在 PATH 中提供 LLVM `clang` 以编译 `ring`。
SDK `MakeAppx` 保持语义验证开启，
不会使用 `/nv` 跳过验证；同一 SDK 的 `MakePri` 生成图标资源索引 `resources.pri`，
SDK 安装时需包含 UWP 工具。MSIX 面向商店上传，不包含本地签名；Microsoft Store
会处理分发签名，因此无需 PFX Secret。直接侧载测试需要另行为测试包签名并信任
测试证书，或使用商店的包航班分发。

## Markdown 右键打开

清单为 `.md`、`.markdown`、`.mdown`、`.mkd` 声明文件关联及 `Ryne.Open` 静态动作，
标签为 `Open with Ryne`，文件参数使用带引号的 `"%1"`。关联及动作均使用
`MultiSelectModel="Single"`，与当前单文件窗口保持一致。安装后可从资源管理器的
“打开方式 → Ryne”打开；静态动作的直接菜单展示受系统版本和当前文件关联影响，
Windows 11 可能需进入“显示更多选项”，本次未增加现代菜单 COM 扩展。
沿用现有命令行文件打开和 Windows 实例投递，已有文档有未保存内容时沿用确认流程。

便携 ZIP 随附 `Register-context-menu.cmd`、`Unregister-context-menu.cmd` 和
`register-context-menu.ps1`，兼容系统自带 Windows PowerShell 5.1。注册仅写当前用户的
`Software\Classes\Applications\Ryne.exe` 和四个扩展名的
`SystemFileAssociations\<extension>\shell\Ryne.Open`，通知 Shell 刷新关联；不修改默认
ProgID 或 UserChoice。命令直接调用带引号的 EXE 和文件路径，支持空格及中文路径。
重复注册更新当前目录；移除时核对命令归属，旧目录的移除脚本不会删掉新目录的注册。
移动或删除便携目录前先移除，移动后重新注册。MSIX 打包会排除这些便携脚本，
关联随包安装和卸载管理。

本地已完成 XML/打包路径及脚本静态检查；当前主机为 macOS，未执行 Windows 注册表或
MakeAppx 验证。Windows 实机验收应覆盖：四种扩展名、中文/空格路径、已打开窗口和
未保存确认、重复注册、换目录注册后从旧目录移除、移除后其他编辑器的默认关联不变，
以及 MSIX 安装/卸载和 Windows 11 的“打开方式”/“显示更多选项”。

## 官方资料

- [GitHub Actions 商店更新](https://learn.microsoft.com/en-us/windows/apps/publish/msstore-dev-cli/github-actions)
- [Store CLI 与首次发布要求](https://learn.microsoft.com/en-us/windows/apps/publish/msstore-dev-cli/overview)
- [MSIX CLI 命令与草稿行为](https://learn.microsoft.com/en-us/windows/apps/publish/msstore-dev-cli/commands)
- [MSIX 签名](https://learn.microsoft.com/en-us/windows/msix/package/sign-msix-package-guide)
- [版本编号规则](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/package-version-numbering?pivots=store-installer-msix)
- [MakeAppx 工具](https://learn.microsoft.com/en-us/windows/msix/package/create-app-package-with-makeappx-tool)
- [上传 MSIX 商店包](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/device-families)
- [文件关联与静态菜单动作](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/desktop-to-uwp-extensions)
- [应用与 SystemFileAssociations 注册](https://learn.microsoft.com/en-us/windows/win32/shell/app-registration)
- [Windows 11 菜单扩展边界](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/integrate-packaged-app-with-file-explorer)
