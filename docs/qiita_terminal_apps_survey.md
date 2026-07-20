---
Title: 2026年のmacOSターミナル事情：GPU加速とAI連携、26アプリの選び方
Tags: macOS, Terminal, Shell, AI, GPU
Platform: Qiita
---

最近、自分でもmacOS向けターミナルアプリ **[Noa](https://github.com/simota/noa)** の開発を始めました。描画方式や端末エミュレーション、タブ・ペイン管理を調べるうちに、「現在のターミナルアプリは、どのような技術で作られ、何を目指しているのか」を一度整理したくなりました。

調べてみると、定番のiTerm2やWezTerm、Ghosttyだけでなく、GPU描画、AIエージェント連携、2Dキャンバス型UIなど、選択肢は想像以上に広がっています。macOSで動作する26アプリを技術スタック、描画方式、設定方法、主要機能、ライセンス、プロジェクト開始年で比較し、用途別の選び方と新興アプリを評価する際の注意点を整理します。

## TL;DR
- **GPU描画の成熟**: Ghostty (Zig + Metal) や Rio (Rust + WebGPU) を筆頭に、低レイテンシで画面ティアリングのない描画が一般化。
- **AI/MCP（Agentic）連携の標準化**: ターミナル自体が MCP サーバーを内蔵し、Claude Code などの自律AIエージェントによる操作・監視をネイティブサポートする流れが加速。
- **UI/UXの多様化**: 入力部を独立したエディタにする Warp や、`xterm.js` の端末を無限の2Dキャンバスに並べる TermCanvas などが台頭。
- **ワークスペースの進化**: `libghostty` で端末を描画し、エージェントの通知やソケットAPI制御を統合する cmux などが登場。

---

## 1. なぜ今、ターミナル環境の「乗り換え」が起きているのか？

これまでmacOSの定番ターミナルといえば、高機能な `iTerm2` や、シンプルで速い `Alacritty` `kitty` などが代表格でした。しかし現在、多くのエンジニアが環境を刷新し始めています。背景には主に2つの要因があります。

1. **技術スタックの世代交代**: 描画処理が従来の OpenGL/CPU レンダリングから、Metal や WebGPU (`wgpu`) などの最新グラフィックス API へと移行。言語も Zig や Rust が主流となり、キー入力から画面描画までのレイテンシが極限まで削られています。
2. **AIエージェント時代の到来**: `Claude Code` や `Codex` などの自律型CLIエージェントの普及により、エージェントの処理状態の監視、複数エージェントの並行制御、セキュアなコマンド実行境界の管理といった「AI協調機能」がターミナル自体に求められるようになっています。

---

## 2. 【目的別】macOSターミナル選定ガイド

26種類の選択肢から自分に最適なツールを選ぶための簡易的なマトリクスです。

*   **「とにかく安定、これまで通りの高機能がほしい」**
    👉 **[iTerm2]** (Python APIやSSH同期など、長年の蓄積による信頼性は随一)
*   **「究極の表示速度と、macOSネイティブの質感を重視したい」**
    👉 **[Ghostty]** (Zig/Metalによる低遅延、ネイティブGUIによる低消費電力)
*   **「設定をコード(Lua)で記述し、複数OSでDotfiles管理したい」**
    👉 **[WezTerm]** (イベント駆動による高度な自動化、SSH/シリアル接続内蔵)
*   **「コマンド入力を快適にし、AIによるサジェストをフル活用したい」**
    👉 **[Warp]** (IDE風入力エディタ、ブロック単位の出力管理、クラウドAI連携)
*   **「複数のAIコーディングエージェントを効率よく並行・自律実行したい」**
    👉 **[cmux]** (端末描画、エージェント通知、API制御を統合したワークスペース)

---

## 3. 2026年最新の「鉄板」ターミナル環境構築スタック

Qiita/Zennのトレンド記事で高い支持を得ている、開発効率を最大化する構成（スタック）の例です。

*   **ターミナル本体**: `Ghostty` (速さ＋ネイティブUI) または `WezTerm` (LuaによるDotfiles管理)
*   **シェル & プロンプト**: `Zsh` (または `fish`) + **`Starship`** (軽量で極めて多機能なモダンプロンプト)
*   **フォント**: `Nerd Fonts` (アイコンやgitブランチ記号を美しく表示するための必須アイテム)
*   **生産性TUI**:
    *   **`LazyGit`**: ターミナル上でGit操作をグラフィカルかつ爆速に完結。
    *   **`mise`**: 各種プログラミング言語のバージョン管理をプロジェクト単位で一元化。
*   **AIアシスタント**: **`Claude Code`** (ターミナル内からファイル編集やビルド、テスト実行を完結させる自律エージェント)

---

## 4. 全26ターミナルエミュレータ比較マトリクス

macOSで動作する主要なターミナルエミュレータの比較表です。アプリ自身が端末エミュレーションエンジンを実装または組み込み、対話的なシェル画面を描画するものに限定しています。SSHクライアント、外部CLIのUIラッパー、既存ターミナル内で動くTUIマルチプレクサは対象外です。「開始年」は、公式リポジトリのデフォルトブランチに残る最古コミットの年を基本とし、公式の沿革やリリース記録でそれ以前の活動を確認できる場合はそちらを優先しました。これは公開情報で確認できるプロジェクト活動の開始年であり、非公開開発の開始年とは限りません。公式情報または公式リポジトリから一意に確認できないものは「不明」としています。開始年のリンクが判定根拠となる一次情報です。なお、Noaは本記事の筆者が開発しているプロジェクトです。当事者性を明示したうえで、他のプロジェクトと同じ一次情報基準で記載しています。

| アプリ名 | 開始年 | 主要開発言語 | 描画方式・レンダラー | 設定ファイル形式 | 特筆すべき機能 | ライセンス |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Terminal.app** [1] | 不明 | Objective-C / Swift | Core Graphics (CPU) | GUI (plist) | macOS標準、極めて軽量な初期フットプリント | クローズド |
| **iTerm2** [2] | [2010年](https://github.com/gnachman/iTerm2/commit/eb4c3c33506b604e35640b35e75883b8b82abea3) | Objective-C / Swift | Metal / CPU | GUI (plist) | Python API, トリガー, シェル統合, インライン画像表示 | GPL v2 |
| **Alacritty** [3] | [2016年](https://github.com/alacritty/alacritty/commit/621776cd94890936b24f3abb8b7ec1f36dad9150) | Rust | OpenGL | TOML | 超高速、シンプル、設定の即時反映、ミニマリズム | Apache 2.0 |
| **Kitty** [4] | [2016年](https://github.com/kovidgoyal/kitty/commit/de072b3b423ad114cc465fa4c11d85a498c76f2f) | Python / C | OpenGL | プレーンテキスト | タブ/ペイン、独自のKittyグラフィックス/キーボードプロトコル | GPL v3 |
| **WezTerm** [5] | [2017年](https://github.com/wezterm/wezterm/commit/c53ca64c33d1658602b9a3aaa412eca9c6544294) | Rust | OpenGL / Metal | Lua (`.lua`) | イベント駆動Lua拡張、SSH/Tramp内蔵、シリアル接続 | MIT |
| **Ghostty** [6] | [2022年](https://github.com/ghostty-org/ghostty/commit/f8b0000444663ade13d75e1e703bbad3cfdd1ce2) | Zig | Metal (macOS) / GTK4 | プレーンテキスト | SwiftネイティブUI、極低レイテンシ、C-ABI互換コア | MIT |
| **Rio** [7] | [2022年](https://github.com/raphamorim/rio/commit/94d055d16ecf1115a0b469faf4ee24a8fc8a02d2) | Rust | WebGPU (Metal/Vulkan/DX12) | TOML | ブラウザ(Wasm)互換、Sixel/Kittyグラフィックス対応 | MIT |
| **Warp** [8] | [2020年](https://www.warp.dev/blog/block-model-behind-warps-agentic-development-environment) | Rust | Metal / 独自GPU | GUI / YAML | AI統合、クラウド共有(Warp Drive)、IDE風入力エディタ | 商用 / 部分OSS |
| **Wave Terminal** [9] | [2023年](https://legacydocs.waveterm.dev/releasenotes) | TypeScript / Go | WebKit / HTML Canvas | GUI / JSON | インラインMarkdown/画像、履歴のカード化、クラウド同期 | Apache 2.0 |
| **Tabby** [10] | [2016年](https://github.com/Eugeny/tabby/commit/98fea7b1023383b4fafe108b0170f68f15831021) | TypeScript | WebGL (Electron) | GUI / YAML | マルチプラットフォーム、SSH/SFTP接続、プラグイン拡張 | Apache 2.0 |
| **Hyper** [11] | [2016年](https://github.com/vercel/hyper/commit/1a1178bd387890a9a9832ab23a541351001b2ecc) | JavaScript | DOM (Electron) | GUI / JS | CSS/JSによる高度な外見拡張、プラグインエコシステム | MIT |
| **Extraterm** [12] | [2014年](https://github.com/sedwards2009/extraterm/commit/2215a276ef0ad4fe250737c61efb602bca4e05de) | TypeScript | DOM (Electron) | GUI / JSON | コマンド出力を「フレーム」で管理、出力のパイプ・編集 | MIT |
| **DomTerm** [13] | [2015年](https://github.com/PerBothner/DomTerm/commit/745e2d99f23adc4a4353a3e5770e8abe09e86af9) | C++ / JavaScript | DOM (Electron / Web) | GUI / JS | HTML/SVG表示、リッチテキスト、コード折りたたみ | BSD 3-Clause |
| **MacTerm (thdxg)** [14] | [2026年](https://github.com/thdxg/macterm/commit/d55e1274340bcb16c996fb555ad8588a5ae8fd79) | Swift | Metal (libghostty) | プレーンテキスト | プロジェクト管理サイドバー、Ghostty設定互換 | MIT |
| **Attyx** [15] | [2026年](https://github.com/semos-labs/attyx/commit/4dcc281aa4fa606941dece92da095a01968d4be4) | Zig | Metal | GUI / テキスト | AIエージェント/MCP統合、ステータス自動監視、超軽量 | MIT |
| **Crux** [16] | [2026年](https://github.com/HarryJhin/crux/commit/496f925d8d31229522170e1bec91100af7864944) | Rust | Metal / GPUI | プレーンテキスト | MCPサーバー内蔵、GPU駆動UI、macOS特化 | MIT |
| **zt** [17] | [2026年](https://github.com/midasdf/zt/commit/f11cb6652ea1c93419c92905b883da3d84620596) | Zig | Cocoa (native) | プレーンテキスト | 極小メモリフットプリント、88MB/s超の高速パース | MIT |
| **Termy** [18] | [2026年](https://github.com/lassejlv/termy/commit/89118b841cf112d463cd1b246f1d6ecd5e712531) | Rust | GPUI (Metal / OpenGL) | プレーンテキスト | GPU加速、tmux連携、タブ/分割、ZedのGPUI採用 | MIT |
| **Noa（筆者開発）** [19] | [2026年](https://github.com/simota/noa/commit/0b0f402b86c9cc8960993f26268c46fe626fb504) | Rust | wgpu (Metal / OpenGL) | プレーンテキスト | Ghostty互換、Kittyグラフィックス、全タブ一覧(Session Overview) | MIT |
| **Rootshell** [20] | [2026年](https://github.com/kitknox/rootshell/commit/da935e8d6ede00997761ceef15a8551c66cbabe4) | Swift / Helix | Metal (libghostty) | プレーンテキスト | iOS/Vision Pro対応、クラウド/K8s統合、Helixエディタ内蔵 | MIT |
| **Conterm** [21] | [2026年](https://github.com/mahdiarfrm/conterm/commit/b482255007f4ccbaf3c35a9c8ce5c164a0d7ee82) | Swift | Metal (libghostty) | プレーンテキスト | 「Liquid Glass」すりガラスUI、コマンドパレット、タブグループ | MIT |
| **jebi** [22] | [2026年](https://github.com/jebi-sh/jebi/commit/07b8392ef278e31c83f1f8b0d3d776cf39d0512b) | SwiftUI / Rust | Metal | プレーンテキスト | ローカルAI（LLM）内蔵、APIキー不要の `/ask` チャット、エラー解説 | MIT |
| **TermCanvas** [23] | [2026年](https://github.com/lout33/termcanvas/commit/a922667417fda0d2f529596da295fdee54d3b2c4) | TypeScript | DOM (Electron) | GUI / JSON | `xterm.js` の実ターミナルを無限2Dキャンバス上に配置。AIエージェントの並行監視 | MIT |
| **Contour** [24] | [2019年](https://github.com/contour-terminal/contour/commit/35da04c3207d9007067acdb48e6a5b37a6f45ee0) | C++ | OpenGL / 独自 | プレーンテキスト | 高機能なモーダル操作（Vim風）、高度なANSI/VTシーケンス対応 | Apache 2.0 |
| **Electerm** [25] | [2017年](https://github.com/electerm/electerm/commit/87cfb297a208df5af7627105a2292732c3e15318) | JS / Electron | WebGL | GUI / JSON | ローカル端末、SSH/SFTP/Serial/RDP/VNC統合、AI統合 | MIT |
| **cmux** [26] | [2026年](https://github.com/manaflow-ai/cmux/commit/c5bd543fe0f9859371efda61d49fde10b4b0f9ba) | Swift / `libghostty` | Metal | GUI / JSON | ターミナル描画、エージェント通知、CLI / socket API、組み込みブラウザ | GPL-3.0-or-later / 商用ライセンス |

---

## 5. 主要アプリの内部アーキテクチャとメリット・デメリット

### 5.1 Ghostty [6]：究極のパフォーマンスを追求するZig/Metalハイブリッド（2022年〜）
*   **メリット (PROs)**:
    *   **Zero-GCによる超低遅延**: Zig言語による手動メモリ管理により、キー入力から描画までのラグを感じない。
    *   **OSネイティブUIの快適さ**: Swiftで書かれたAppKitネイティブのウィンドウやメニューを持ち、Electron製やクロスプラットフォームツールに比べてメモリ消費量が極小。
*   **デメリット (CONs)**:
    *   **設定GUIの不在**: 設定は完全にテキストファイルで行うため、GUIでの直感的な調整を好むユーザーには不向き。
*   **技術の深掘り**: 公式リポジトリの履歴は2022年に始まり、2024年末に一般公開されました。コアを `libghostty` として分離。描画パイプラインには macOS ネティブの Metal API を使用し、GPUに描画負荷をオフロードすることで、CPU使用率と発熱を最小限に抑えています。

### 5.2 Warp [8]：IDEレベルの操作性とクラウドAIを融合（2020年〜）
*   **メリット (PROs)**:
    *   **コードエディタ並みの入力欄**: マウスでのカーソルクリック、ブラケット自動補完、複数行編集などを標準でサポート。
    *   **コマンド/出力のブロック化**: コマンドの出力結果を個別の「ブロック」として扱え、右クリックでブロック内容の共有やAI解析が瞬時に行える。
*   **デメリット (CONs)**:
    *   **クラウド依存**: オフラインで動作するものの、高度なAI機能やチーム共有スペース「Warp Drive」の利用にはネットワーク接続が必須。
*   **技術の深掘り**: 2020年に設立・開発が始まり、2022年に公開。入力欄を単なるターミナルの文字入力バッファではなく、リッチテキストウィジェットとして完全に独立させています。

### 5.3 WezTerm [5]：Luaスクリプトによるイベント駆動型カスタマイズ（2017年〜）
*   **メリット (PROs)**:
    *   **完全なプログラマビリティ**: 設定ファイルをフル機能の Lua スクリプトとして記述可能。
    *   **強力なマルチプレクサ機能**: `tmux` を使わずに、WezTerm 自体で強力なセッションのデタッチ/アタッチ、ペイン分割を制御できる。
*   **デメリット (CONs)**:
    *   **設定の難易度**: Lua の構文に慣れていない場合、設定ファイルを記述するハードルが高い。
*   **技術の深掘り**: 公式リポジトリでは2017年から開発されているRust製。`wezterm.on` を用いたイベントフックの例です。以下を設定に追加するだけで、ステータスバーを動的に書き換えられます。

```lua
local wezterm = require 'wezterm'
local config = wezterm.config_builder()

wezterm.on('update-status', function(window, pane)
  local date = wezterm.strftime('%Y-%m-%d %H:%M:%S')
  local workspace = window:active_workspace()
  window:set_right_status(wezterm.format({
    { Text = 'Workspace: ' .. workspace .. ' | ' .. date },
  }))
end)

return config
```

### 5.4 Rio [7]：RustとWebGPUで実現するクロスプラットフォーム描画（2022年〜）
*   **メリット (PROs)**:
    *   **WebGPUのネイティブ活用**: `wgpu` クレートにより、macOS (Metal)、Linux (Vulkan)、Windows (DirectX12) の各低レベルAPIへ自動最適化。
    *   **ブラウザ動作の将来性**: WebAssemblyビルドを意識しており、ブラウザ上で同一の描画アルゴリズムを動かすことが視野に入っている。
*   **デメリット (CONs)**:
    *   **発展途上の機能性**: iTerm2やWezTermのような長年の開発で培われた高度な機能（詳細なキーマッピング、スクリプトAPIなど）はまだ少ない。

### 5.5 iTerm2 [2]：歴史ある安定性と近代化機能の調和（2010年〜）
*   **メリット (PROs)**:
    *   **圧倒的な機能の網羅性**: トリガー、パスワードマネージャ、シェルインテグレーション、外部APIなど、必要となる機能はほぼ全て実装済み。
    *   **Python APIによる自動化**: 強力なPython APIを介して、ターミナルセッションをプログラムから容易に制御。
*   **デメリット (CONs)**:
    *   **機能肥大化 (Bloat) によるオーバーヘッド**: メモリ使用量が多く、特に無限スクロールバックを使用するとギガバイト単位でリソースを消費する傾向。

---

## 6. 特化・新興アプリの選定ガイド

### 6.1 接続機能を統合したターミナルエミュレータ
*   **Electerm** [25]: 2017年開発開始。ローカル端末に加えてSSH/SFTP/FTP/Telnet/Serial/RDP/VNC/Spiceに対応したElectron製の多機能ターミナル兼ファイルマネージャ。

### 6.2 HTML/DOMベースの独自UIエミュレータ
*   **Extraterm** [12]: 2014年開発開始。コマンドの出力結果を「フレーム（枠）」に収め、そのフレーム内のコンテンツをインプレースで編集・検索したり、別のコマンドに直接パイプしたりできるエディタ風ターミナル。
*   **DomTerm** [13]: 2015年開発開始。ブラウザの DOM ツリーを利用して HTML/SVG の動的埋め込み、数式やグラフのインライン表示、出力をフォルダのように折りたたむ（Folding）機能をサポート。
*   **TermCanvas** [23]: 2026年開発開始。`xterm.js` で描画する対話的なシェル端末を2Dの無限キャンバス上のノードとして並行管理。AIエージェントの進行監視やセッション復元にも対応。

### 6.3 2026年注目される新興・AIネイティブ/GPU加速アプリ
*   **MacTerm (thdxg/macterm)** [14]: 公式リポジトリでは2026年開発開始。`libghostty` をバックエンドに採用したSwift製macOS専用ターミナル。プロジェクトごとにディレクトリを管理できる縦型サイドバー、永続的なタブ・ペイン管理を装備。
*   **Termy (lassejlv/termy)** [18]: 2026年開発開始。ZedエディタのUIフレームワークである **GPUI** と `alacritty_terminal` エンジンを採用したRust製GPU加速ターミナル。
*   **Noa (simota/noa、筆者開発)** [19]: 筆者自身が開発しているプロジェクトで、公式リポジトリでは2026年開発開始。Rust/winit/wgpuを使用したGPU加速ターミナル。Ghostty互換設計で、全タブを一望する「Session Overview」や背景画像の動的切り替えに対応。開発者本人による記述であるため、導入判断ではリポジトリの実装状況やリリースをあわせて確認してください。
*   **Attyx / Crux** [15][16]: いずれも公式リポジトリでは2026年開発開始。AIエージェントとの協調（Agentic Workflows）を前提としていますが、現行機能とロードマップ上の計画は分けて評価する必要があります。
*   **zt** [17]: 2026年開発開始。Zig製 Cocoaネイティブの極小・超軽量ターミナル。毎秒88MB超のパース速度と極限の省メモリを追求。
*   **Rootshell** [20]: 2026年開発開始。`libghostty` 搭載のAppleデバイス（iOS/Vision Pro/macOS）向けターミナル。Secure Enclave鍵管理、耐量子SSH、QUIC+KCPベースのセッション持続プロトコル "Rootshell Roam"、Helixエディタのネイティブ移植を内蔵。
*   **Conterm** [21]: 2026年開発開始。`libghostty` ベースのmacOS専用アプリ。透過やブラーを動的調整する「Liquid Glass」UI、再帰的スプリットペイン、カラーコード化タブ、セッション復元、操作音を搭載。
*   **jebi** [22]: 2026年開発開始。SwiftUI/Rust構成で、ローカルAIモデルを直接内蔵。外部API接続やAPIキー不要で動作し、エラー自動解説、次に叩くべきコマンドの提案、AIチャット画面を標準装備。
*   **Contour** [24]: 2019年開発開始。C++製マルチプラットフォーム（Linux/macOS/Windows）モーダル（Vim風）ターミナル。GPU描画、Truecolor、リガチャー対応。キーボード駆動でハッシュ等を選択する「Hintモード」、OSC 8 / OSC 52 / Sixel表示対応。
*   **cmux** [26]: 2026年開発開始。`libghostty` で端末を描画するmacOSネイティブアプリ。並行エージェントを追跡する通知、サイドバー、CLI / socket API、組み込みブラウザを提供。

---

## 7. 2026年、新興アプリを選定する際の「4つの罠」

魅力的な次世代機能が数多く並びますが、実際の日常業務に採用する際には冷徹な見極めが必要です。

### ① 「計画中（Roadmap）」と「実装済み」の混同
ドキュメントで「MCP対応」「マルチペイン」と謳われていても、実際には計画段階（Issues/Roadmap）であり、現行のバイナリには未実装のプロジェクトが散見されます（例: 開発初期のCruxなど）。

### ② ベンチマーク数値の測定環境を疑う
「従来の◯倍高速」という主張の多くは、画面の描画更新やPTY読み込みなどの特定コンポーネントを切り出したプロジェクト独自のテストによるものです。フォント、Unicode、スクロールバックの保持ポリシーなど、実際のワークロードでの評価が不可欠です。

### ③ `libghostty` 採用＝アプリとしての完成度ではない
MactermやRootshell、cmuxは `libghostty` をエンジンとして活用しています。描画やエミュレーションのコアが強固であっても、macOS上での署名（ゲートキーパー）、自動アップデート、ウインドウ・セッション管理、設定GUIなど、「アプリケーション全体としての品質」は個々のプロジェクトに依存します。

### ④ AI/エージェント連携におけるセキュリティ境界
エージェント連携に対応したエミュレータ（cmux、TermCanvas）を導入する際は、以下の基準で別途厳格に評価されるべきです。
1. **セッション永続化の仕様**: 再起動時などの復旧パターン
2. **API/ソケットの認証**: 他のローカルプロセスから不正制御されないか
3. **データ送信のポリシー**: 機密性の高いプロジェクトコードが外部送信されないか

---

## 8. まとめ

開発者それぞれのスタイルやニーズに合わせて、以下のように選定するのが2026年現在のベストプラクティスです。

- **歴史的安定性と手厚いインフラ管理がほしい**: `iTerm2` (Python API や SSH 連携の熟成度は圧倒的)
- **究極の低レイテンシとミニマリズムを追求**: `Ghostty` または `WezTerm`
- **コマンド入力を快適にし、ブロック単位の操作やクラウドAIを活用したい**: `Warp`
- **複数のAIエージェントとブラウザを統合したワークスペースを構築したい**: `cmux`

# CTA Block
あなたのお気に入りのターミナル環境や、AIエージェントとの組み合わせがあれば、ぜひコメント欄で教えてください！
まずは手元で軽量かつ高性能な **[Ghostty](https://ghostty.org/)** または Lua で自在にハックできる **[WezTerm](https://wezterm.org/)** をインストールし、ターミナルの進化を体験してみませんか？

---

## 9. 一次情報・参考文献 (Sources)

*   [1] [macOS Terminal - Apple Support](https://support.apple.com/guide/terminal/welcome/mac)（開始年を特定できる公式記載なし）
*   [2] [iTerm2 - Official Website](https://iterm2.com/)
*   [3] [Alacritty - GitHub](https://github.com/alacritty/alacritty)
*   [4] [Kitty - Official Website](https://sw.kovidgoyal.net/kitty/)
*   [5] [WezTerm - Official Website](https://wezfurlong.org/wezterm/)
*   [6] [Ghostty - Official Website](https://ghostty.org/)
*   [7] [Rio - GitHub](https://github.com/raphamorim/rio)
*   [8] [Warp - Official Website](https://www.warp.dev/)
*   [9] [Wave Terminal - Official Website](https://www.waveterm.dev/)
*   [10] [Tabby - Official Website](https://tabby.sh/)
*   [11] [Hyper - Official Website](https://hyper.is/)
*   [12] [Extraterm - Official Website](https://extraterm.org/)
*   [13] [DomTerm - Official Website](https://domterm.org/)
*   [14] [MacTerm (thdxg/macterm) - GitHub](https://github.com/thdxg/macterm)
*   [15] [Attyx - GitHub](https://github.com/semos-labs/attyx)
*   [16] [Crux - GitHub](https://github.com/HarryJhin/crux)
*   [17] [zt - GitHub](https://github.com/midasdf/zt)
*   [18] [Termy - GitHub](https://github.com/lassejlv/termy)
*   [19] [Noa - GitHub](https://github.com/simota/noa)
*   [20] [Rootshell - GitHub](https://github.com/kitknox/rootshell)
*   [21] [Conterm - GitHub](https://github.com/mahdiarfrm/conterm)
*   [22] [jebi - GitHub](https://github.com/jebi-sh/jebi)
*   [23] [TermCanvas - GitHub](https://github.com/lout33/termcanvas)
*   [24] [Contour - GitHub](https://github.com/contour-terminal/contour)
*   [25] [Electerm - GitHub](https://github.com/electerm/electerm)
*   [26] [cmux - GitHub](https://github.com/manaflow-ai/cmux)
