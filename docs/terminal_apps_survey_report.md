# macOS向けターミナルエミュレータ調査レポート（2026年7月版）

macOSで利用できるターミナルエミュレータを比較します。対象読者は、日常利用またはAIコーディング用途のターミナル環境を選定する開発者です。

## 1. 調査範囲と読み方

- 調査基準日は2026年7月17日です。
- 機能、対応OS、ライセンスは、原則として公式サイト、公式ドキュメント、公式リポジトリで確認しました。
- アプリ自身が端末エミュレーションエンジンを実装または組み込み、対話的なシェル画面を描画するものを対象とします。SSHクライアント、外部CLIのUIラッパー、既存ターミナル内で動くTUIマルチプレクサは除外します。
- GitHub Starsは変動が大きく、商用製品との比較にも適さないため、選定指標から外しました。
- 「開始年」は、公式リポジトリのデフォルトブランチに残る最古コミットの年を基本とし、公式の沿革やリリース記録でそれ以前の活動を確認できる場合はそちらを優先しました。これは公開情報で確認できるプロジェクト活動の開始年であり、非公開開発の開始年とは限りません。
- 公式情報または公式リポジトリから開始年を一意に確認できないものは「不明」としました。開始年のリンクは判定根拠となる一次情報です。
- Noaは本レポートの筆者が開発しているプロジェクトです。当事者性を明示したうえで、他のプロジェクトと同じ一次情報基準で記載しています。
- 性能に関する記述は各プロジェクトの設計方針を要約したものです。本レポートでは統一条件の独立ベンチマークを実施していません。
- 「計画中」は公式ロードマップ上の項目であり、現行版で利用できる機能ではありません。

## 2. 比較マトリクス

### 2.1 汎用ターミナルエミュレータ

| アプリ | 開始年 | 主な実装・描画方式 | 設定 | 主な特徴 | ライセンス |
| :--- | :--- | :--- | :--- | :--- | :--- |
| [Terminal.app](https://support.apple.com/guide/terminal/welcome/mac) | 不明 | macOSネイティブ | GUI / plist | macOS標準、追加導入不要 | Apple独自 |
| [iTerm2](https://iterm2.com/) | [2010年](https://github.com/gnachman/iTerm2/commit/eb4c3c33506b604e35640b35e75883b8b82abea3) | Objective-C / Swift、Metal対応 | GUI / plist | プロファイル、シェル統合、Python API、インライン画像 | GPL-2.0 |
| [Alacritty](https://github.com/alacritty/alacritty) | [2016年](https://github.com/alacritty/alacritty/commit/621776cd94890936b24f3abb8b7ec1f36dad9150) | Rust、OpenGL | TOML | 最小限のUI、クロスプラットフォーム、設定の再読み込み | Apache-2.0 |
| [kitty](https://github.com/kovidgoyal/kitty) | [2016年](https://github.com/kovidgoyal/kitty/commit/de072b3b423ad114cc465fa4c11d85a498c76f2f) | C / Python / Go、OpenGL | テキスト | タブ・分割、Kitty graphics protocol、Kitty keyboard protocol | GPL-3.0 |
| [WezTerm](https://github.com/wezterm/wezterm) | [2017年](https://github.com/wezterm/wezterm/commit/c53ca64c33d1658602b9a3aaa412eca9c6544294) | Rust、GPUアクセラレーション | Lua | Luaイベント、タブ・分割、SSH domain、シリアル接続 | MIT |
| [Ghostty](https://github.com/ghostty-org/ghostty) | [2022年](https://github.com/ghostty-org/ghostty/commit/f8b0000444663ade13d75e1e703bbad3cfdd1ce2) | Zigコア、macOSはSwift/AppKit・SwiftUIとMetal | `key = value` 形式 | ネイティブUI、タブ・分割、Kitty graphics protocol、`libghostty` | MIT |
| [Rio](https://github.com/raphamorim/rio) | [2022年](https://github.com/raphamorim/rio/commit/94d055d16ecf1115a0b469faf4ee24a8fc8a02d2) | Rust、`wgpu` | TOML | macOS・Linux・Windows向けGPU描画、Sixel・Kitty画像対応 | MIT |
| [Warp](https://github.com/warpdotdev/warp) | [2020年](https://www.warp.dev/blog/block-model-behind-warps-agentic-development-environment) | Rust、GPU描画 | GUI / YAML | ブロック型UI、編集可能な入力欄、AI・共有機能 | AGPL-3.0（UI関連crateはMIT） |
| [Wave Terminal](https://github.com/wavetermdev/waveterm) | [2023年](https://legacydocs.waveterm.dev/releasenotes) | TypeScript / Go、Web技術 | GUI / JSON | ブロック型ワークスペース、インライン表示、AI・ウィジェット | Apache-2.0 |
| [Tabby](https://github.com/Eugeny/tabby) | [2016年](https://github.com/Eugeny/tabby/commit/98fea7b1023383b4fafe108b0170f68f15831021) | TypeScript、Electron | GUI / YAML | SSH・SFTP・Serial、分割、プラグイン | MIT |
| [Hyper](https://github.com/vercel/hyper) | [2016年](https://github.com/vercel/hyper/commit/1a1178bd387890a9a9832ab23a541351001b2ecc) | JavaScript、Electron / DOM | `.hyper.js` | CSS・JavaScriptによる外観とプラグインの拡張 | MIT |
| [Extraterm](https://github.com/sedwards2009/extraterm) | [2014年](https://github.com/sedwards2009/extraterm/commit/2215a276ef0ad4fe250737c61efb602bca4e05de) | TypeScript、Electron | GUI / JSON | コマンド出力をフレーム単位で管理 | MIT |
| [DomTerm](https://github.com/PerBothner/DomTerm) | [2015年](https://github.com/PerBothner/DomTerm/commit/745e2d99f23adc4a4353a3e5770e8abe09e86af9) | C++ / JavaScript、Web / DOM | GUI / JavaScript | HTML・SVG表示、出力の折りたたみ | 公式リポジトリ参照 |
| [Contour](https://github.com/contour-terminal/contour) | [2019年](https://github.com/contour-terminal/contour/commit/35da04c3207d9007067acdb48e6a5b37a6f45ee0) | C++、GPU描画 | YAML | Vi風入力、Hintモード、Sixel、各種OSC対応 | Apache-2.0 |

### 2.2 新興・実験的なターミナルエミュレータ

| アプリ | 開始年 | 実装・位置づけ | 確認できる特徴 | 現時点の注意 | ライセンス |
| :--- | :--- | :--- | :--- | :--- | :--- |
| [Macterm](https://github.com/thdxg/macterm) | [2026年](https://github.com/thdxg/macterm/commit/d55e1274340bcb16c996fb555ad8588a5ae8fd79) | Swift + `libghostty`、macOS専用 | プロジェクトサイドバー、永続セッション、Ghostty設定互換 | 署名状況と導入手順を確認する必要がある | MIT |
| [Attyx](https://github.com/semos-labs/attyx) | [2026年](https://github.com/semos-labs/attyx/commit/4dcc281aa4fa606941dece92da095a01968d4be4) | Zig、Metal / OpenGL | タブ・分割、セッション、IPC / CLI / MCP、エージェント状態表示 | 新興プロジェクトのため互換性と運用実績を要確認 | MIT |
| [Crux](https://github.com/HarryJhin/crux) | [2026年](https://github.com/HarryJhin/crux/commit/496f925d8d31229522170e1bec91100af7864944) | Rust + GPUI / Metal、macOS専用 | 基本的なターミナル描画とVTエミュレーション | 初期開発段階。ペイン、IPC、MCPはロードマップ上の計画 | MIT / Apache-2.0 |
| [zt](https://github.com/midasdf/zt) | [2026年](https://github.com/midasdf/zt/commit/f11cb6652ea1c93419c92905b883da3d84620596) | Zig、CPU描画 | 小さな実装、Linux framebuffer・X11・Wayland対応 | macOSバックエンドはCocoa/AppKitで未テスト。Metal描画ではない | MIT |
| [Termy](https://github.com/lassejlv/termy) | [2026年](https://github.com/lassejlv/termy/commit/89118b841cf112d463cd1b246f1d6ecd5e712531) | Rust + GPUI | タブ・分割、検索、レイアウト、任意のtmux連携 | macOS配布物の署名状況を確認する必要がある | MIT |
| [Noa（筆者開発）](https://github.com/simota/noa) | [2026年](https://github.com/simota/noa/commit/0b0f402b86c9cc8960993f26268c46fe626fb504) | Rust + `winit` / `wgpu` | タブ・分割、Kitty画像、Session Overview | 初期開発段階。機能と互換性は対象リビジョンで確認が必要 | MIT |
| [Rootshell](https://github.com/kitknox/rootshell) | [2026年](https://github.com/kitknox/rootshell/commit/da935e8d6ede00997761ceef15a8551c66cbabe4) | Appleプラットフォーム向け、`libghostty` 利用 | 公式READMEではSSH、クラウド連携、エディタ統合を掲示 | 対応済み機能と計画中機能、ライセンス条件を個別確認する必要がある | 明示確認できず |
| [Conterm](https://github.com/mahdiarfrm/conterm) | [2026年](https://github.com/mahdiarfrm/conterm/commit/b482255007f4ccbaf3c35a9c8ce5c164a0d7ee82) | Swift + `libghostty`、macOS専用 | Liquid Glass風UI、分割、タブグループ、コマンドパレット | 新興プロジェクトのため配布・互換性を要確認 | MIT |
| [jebi](https://github.com/jebi-sh/jebi) | [2026年](https://github.com/jebi-sh/jebi/commit/07b8392ef278e31c83f1f8b0d3d776cf39d0512b) | SwiftUI / Rust、macOS専用 | ローカルAIによる提案、エラー解説、`/ask` | モデル要件、リソース消費、対応ハードウェアを要確認 | MIT |

### 2.3 複合型ターミナルエミュレータ

| アプリ | 開始年 | 種別 | 主な用途 | ライセンス・提供形態 |
| :--- | :--- | :--- | :--- | :--- |
| [Electerm](https://github.com/electerm/electerm) | [2017年](https://github.com/electerm/electerm/commit/87cfb297a208df5af7627105a2292732c3e15318) | リモート接続クライアント | SSH、SFTP、Serial、RDP、VNCなどの統合 | MIT |
| [TermCanvas](https://github.com/lout33/termcanvas) | [2026年](https://github.com/lout33/termcanvas/commit/a922667417fda0d2f529596da295fdee54d3b2c4) | 空間ワークスペース | `xterm.js` の実ターミナルを無限キャンバスへ配置し、tmuxやAIエージェントを管理 | MIT |
| [cmux](https://github.com/manaflow-ai/cmux) | [2026年](https://github.com/manaflow-ai/cmux/commit/c5bd543fe0f9859371efda61d49fde10b4b0f9ba) | macOS向けターミナルワークスペース | `libghostty`、縦タブ、通知、組み込みブラウザ、CLI / socket API | GPL-3.0-or-later / 商用ライセンス |

cmux本体はSwift/AppKit製のmacOSアプリです。Linux向けのRust/GTK4実装は別プロジェクトの[limux](https://github.com/am-will/limux)であり、cmuxのLinux版ではありません。

## 3. 主要アプリの技術的特徴

### 3.1 Ghostty：ZigコアとOSネイティブUIの分離（2022年〜）

GhosttyのコアはZigで実装され、C ABIを公開する`libghostty`として構成されています。macOSアプリはSwift、AppKit、SwiftUIでUIを構築し、Metalでターミナル画面を描画します。Linux版はGTK4を使用します。

この構成の要点は、ターミナルエミュレーション、フォント処理、描画を共通コアへ置きつつ、タブや分割、ダイアログなどを各OSのUIで提供できることです。Metalの採用はGPUアクセラレーションを可能にしますが、それだけでCPU使用率、消費電力、入力遅延、ティアリングが常に改善されるとは限りません。性能比較には同一ハードウェアと同一ワークロードでの測定が必要です。

### 3.2 Warp：編集可能な入力UIとクラウド機能（2020年〜）

Warpは入力欄を独立したエディタとして扱い、複数行編集、カーソル移動、補完、構文表示などを提供します。コマンドと出力をブロック単位で扱う点も、従来型ターミナルとの大きな違いです。

2026年4月にクライアントコードが公開されました。リポジトリの大部分はAGPL-3.0で、UI関連crateはMITです。AIモデルのルーティング、課金、会話状態などのクラウド側機能はクライアントリポジトリには含まれません。

初回起動にはインターネット接続が必要です。初期設定後はログアウト状態またはネットワーク遮断状態でも基本的なターミナル機能を利用できます。一方、Warp AI、Warp Drive、Teams、セッション共有などはオンライン接続を必要とします。Warpの公式説明では、明示的な「オフラインモード」が存在するわけではありません。

### 3.3 WezTerm：Luaによるイベント駆動設定（2017年〜）

WezTermは設定ファイル自体をLuaプログラムとして評価します。静的な設定だけでなく、イベントを受けてステータスやキーバインドを変更できます。

次の例は、現在のワークスペース名と時刻を右側のステータスへ表示する最小構成です。

```lua
local wezterm = require 'wezterm'
local config = wezterm.config_builder()

wezterm.on('update-status', function(window, _pane)
  local workspace = window:active_workspace()
  local date = wezterm.strftime('%Y-%m-%d %H:%M:%S')

  window:set_right_status(wezterm.format({
    { Text = 'Workspace: ' .. workspace .. ' | ' .. date },
  }))
end)

return config
```

Lua設定は柔軟ですが、設定がプログラムになるため、構文エラー、API変更、処理時間の長いイベントハンドラに注意が必要です。

### 3.4 Rio：`wgpu`によるクロスプラットフォーム描画（2022年〜）

RioはRustの`wgpu`を使用し、macOSではMetal、LinuxではVulkanまたはOpenGL、WindowsではDirectX系APIなどのバックエンドを利用します。OSごとに完全に別の描画実装を持つ場合と比べ、描画コードを共有しやすい点が利点です。

公式READMEはブラウザを対象プラットフォームとして掲げていますが、Rio本体のWebAssembly移植は未完了です。したがって、現行版を「ブラウザ互換」と評価するのは適切ではありません。

### 3.5 iTerm2：機能範囲と自動化（2010年〜）

iTerm2はプロファイル、トリガー、シェル統合、インライン画像、SSH Integrationなどを備えます。Python APIでは、ウィンドウ、タブ、セッション、ステータスバーなどを外部スクリプトから操作できます。

機能が多い分、設定項目と運用上の選択肢も多くなります。メモリ使用量や描画性能を比較する場合は、スクロールバック上限、フォント、透明度、画像、同時ペイン数を揃える必要があります。「高機能だから遅い」「GPUターミナルだから軽い」と一律には判断できません。

## 4. 新興アプリを評価するときの注意点

### 4.1 実装済み機能とロードマップを分ける

Attyxは公式README上でIPC、CLI、MCP、エージェント状態表示を現行機能として説明しています。一方、Cruxは初期開発段階で、タブ・分割、IPC、MCPを今後のフェーズとして掲げています。同じ「AIエージェント向けターミナル」でも、利用可能な範囲は異なります。

### 4.2 ベンチマーク値は測定条件を確認する

ztが掲げるスループットや起動時間は、プロジェクト独自の測定結果です。実利用では、PTY入出力、Unicode処理、フォント描画、スクロールバック、ウィンドウシステムが結果へ影響します。数値を他アプリと比較する場合は、テストデータと実行環境を揃える必要があります。

### 4.3 `libghostty`採用だけで完成度は判断できない

Macterm、Rootshell、Conterm、cmuxなどは`libghostty`を利用しています。共通コアを利用していても、セッション管理、SSH、設定互換、アクセシビリティ、署名・公証、アップデート機構は各アプリ固有です。導入時はターミナルコア以外の品質も確認する必要があります。

### 4.4 複合型エミュレータは付加機能を別軸で比較する

cmuxとTermCanvasは、それぞれ`libghostty`と`xterm.js`で端末を描画するため本レポートの対象です。一方、純粋な描画性能よりも、複数セッションの可視化、通知、永続化、CLI / socket API、エージェント連携を重視します。通常のターミナルエミュレータと比較するときは、次の項目を分けて評価する必要があります。

- ターミナル互換性と描画品質
- プロセス・セッションの永続化方式
- エージェント状態の検出方法
- ローカルAPIの認証・権限境界
- クラウド送信の有無とプライバシー設定
- コード署名、公証、自動アップデート
- オープンソースライセンスと商用利用条件

## 5. 総括

macOSのターミナル環境には、次の異なる方向性があります。

- Alacritty、kitty、WezTerm、Ghostty、Rioは、GPU描画、互換性、設定性のバランスが異なる汎用ターミナルです。
- iTerm2は、長期間蓄積されたmacOS向け機能と自動化APIを重視する選択肢です。
- WarpとWave Terminalは、ブロック型UIやAI機能を含む開発環境へ範囲を広げています。
- Attyx、Macterm、Termyなどは、エージェント操作やプロジェクト管理を含む新しいワークフローを試みています。
- cmuxとTermCanvasは、端末エミュレーションに加えて複数エージェントの整理と監視に重点を置きます。

選定時は、「GPU対応」や「AI対応」というラベルだけでなく、現行版で利用できる機能、ライセンス、配布物の署名、セッション永続化、ネットワーク通信、日常的に必要な端末互換性を確認することが重要です。

## 6. 主要な一次情報

- [Ghostty: About](https://ghostty.org/docs/about)
- [Ghostty: Features](https://ghostty.org/docs/features)
- [Warp: Open-source client and licensing](https://github.com/warpdotdev/warp)
- [Warp: Using Warp Offline](https://docs.warp.dev/support-and-community/troubleshooting-and-support/using-warp-offline)
- [WezTerm documentation](https://wezterm.org/)
- [Rio repository and platform status](https://github.com/raphamorim/rio)
- [Crux repository and roadmap](https://github.com/HarryJhin/crux)
