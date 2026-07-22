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

初期版では、コンピューター名、ユーザー名、ネットワークアドレス、プロダクトキーを収集対象としていません。ただし、`collection.json`にはデバイスインスタンスID、モデル名、ドライバー情報などが含まれます。機種によっては機器や環境の識別につながる可能性があるため、収集成果物は機密情報を含む可能性のある診断資料として扱ってください。HTMLレポートにはデバイスインスタンスIDを表示しません。

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
