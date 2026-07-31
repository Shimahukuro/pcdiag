# pcdiag

Windows PCの状態を収集・診断し、HTMLレポートを生成するRust製のポータブルCLIツールです。

PC修理や保守の現場で、インストールせずに1コマンドで診断資料を作成することを目的としています。

> 現在は初期開発版です。診断結果は、取得できた情報と実装済みの診断規則の範囲に基づきます。

## 主な機能

- Windows、CPU、メモリ、BIOS・UEFI情報の収集
- GPUとドライバー情報の収集
- 現在および過去に接続されたデバイス情報の収集
- 物理ディスク、パーティション、ボリューム情報の収集
- NVMe Health InformationおよびWindows Failure Predictionの収集
- 収集結果に対する診断規則の実行
- 外部通信や外部リソースを使用しない単一HTMLレポートの生成
- マニフェスト、ファイルサイズ、SHA-256による成果物の整合性検証

コンピューター名、ユーザー名、ネットワークアドレス、プロダクトキーは直接の収集対象としていません。ただし、`collection.json`にはWindowsイベントログ、デバイスインスタンスID、モデル名、ドライバー情報などが含まれます。イベント本文にはアカウント名、パス、端末名などが含まれる場合があります。収集成果物は機密情報を含む可能性のある診断資料として扱ってください。HTMLレポートにはデバイスインスタンスIDを表示しません。

Windowsイベントログは、既定で過去30日のSystem、Application、Securityを収集します。期間を変更する場合は、実行前に環境変数`PCDIAG_EVENT_LOG_DAYS`へ1～3650の日数を設定してください。各ログは新しいものから最大1000件の高優先度イベントを保存します。Securityログの読み取りには管理者権限が必要になる場合があります。

収集したイベントは`collection.json`へ保持し、診断時に予期しないシャットダウン、ストレージI/O障害、サービス障害、アプリケーション異常終了、監査ログ消去、ログオン失敗、監査ポリシー変更へ絞り込みます。同種イベントは件数と最新事例へ集約し、イベント41と6008が60秒以内に記録された場合は同じ異常終了として推定回数を算出します。DCOM 10016など単独では障害を示さない頻出イベントは検出事項に含めません。HTMLレポートでは、収集件数と集約後の検出事項をアコーディオン表示します。

機微情報へのアクセス、成果物の管理責任および新しいコレクターを追加するときの基準は、[`docs/sensitive-data-policy.md`](docs/sensitive-data-policy.md)を参照してください。

## 実行方法

すべての処理を一括実行します。

```powershell
pcdiag.exe
```

出力先を指定する場合は次のように実行します。

```powershell
pcdiag.exe --output D:\pcdiag-results
```

次の処理が順番に実行されます。

```text
collect → diagnose → report
```

各処理は個別にも実行できます。

```powershell
pcdiag.exe collect --output D:\pcdiag-results
pcdiag.exe diagnose --output D:\pcdiag-results\pcdiag-YYYYMMDD-HHMMSS-ID
pcdiag.exe report --output D:\pcdiag-results\pcdiag-YYYYMMDD-HHMMSS-ID
```

## 出力

```text
pcdiag-YYYYMMDD-HHMMSS-ID/
├── collection/
│   ├── collection.json
│   ├── status.json
│   └── manifest.json
├── diagnosis/
│   ├── diagnosis.json
│   └── manifest.json
└── report/
    ├── report.html
    └── manifest.json
```

`report.html`は一般的なWebブラウザーで開けます。

## 権限

通常権限でも実行でき、取得可能な範囲で成果物を生成します。SMARTなど管理者権限が必要な情報は、管理者として起動したターミナルで実行した場合にのみ取得します。取得できなかった値と理由は`status.json`およびHTMLレポートへ記録されます。

## ビルド

Rust stableとWindows MSVCツールチェーンを使用します。

```powershell
cargo build --release -p pcdiag
```

実行ファイルは次の場所に生成されます。

```text
target\release\pcdiag.exe
```

テストと静的検査は次のコマンドで実行します。

```powershell
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

## CI

GitHub Actionsはpush、Pull Requestおよび手動実行を契機として、次を確認します。

- Ubuntu: `cargo fmt`、ワークスペース全体のテスト、Clippy
- Windows: ワークスペース全体のテスト、Releaseビルド、`pcdiag.exe --help`による起動確認

CIでは依存関係を`Cargo.lock`に固定して実行します。Windows固有APIの実デバイスに対する動作や管理者権限による差は、CIではなくWindows実機で確認します。

## Workspace

```text
crates/
├── pcdiag/          CLI、成果物生成、HTMLレポート
├── pcdiag-core/     共通のデータ仕様、検証、診断規則
└── pcdiag-windows/  Windows固有の情報収集
```

## 現在の制約

- 対象OSはWindows 10およびWindows 11です。
- PDFファイルの直接生成には対応していません。
- ATA SMART属性のメーカー・機種別解釈には対応していません。
- USB変換ブリッジなどのベンダー固有SMARTプロトコルには対応していません。
- 自動修復、常駐監視、リモート管理は行いません。

詳しい仕様は[`docs`](docs/)を参照してください。

## バグ報告とセキュリティ

バグを報告する前に、Issue templateの注意事項を確認してください。診断セッション、JSON、HTML、ログ、画面画像には実機を識別できる情報が含まれる可能性があるため、未確認のまま公開Issueへ添付しないでください。

セキュリティ上の問題は公開Issueへ投稿せず、[`SECURITY.md`](SECURITY.md)に従って非公開で報告してください。コントリビューションについては[`CONTRIBUTING.md`](CONTRIBUTING.md)を参照してください。

## License

Apache License 2.0
