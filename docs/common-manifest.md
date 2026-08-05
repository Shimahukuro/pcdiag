# 共通マニフェスト仕様

## 目的

`pcdiag`が生成する収集、診断、レポートの各成果物について、作成元、形式、処理状態、入力関係、構成ファイルを共通の方法で記録する。

共通マニフェストは成果物を管理・検証するためのメタデータであり、検査対象PCから収集した診断情報を保存する場所ではない。

検査対象PCのシステム時計、Windows Timeサービス、時刻同期状態、ハードウェアRTCなどの情報は検査項目として`collection.json`へ保存し、本マニフェストには含めない。

## 配置単位

初期実装では、各成果物ディレクトリに`manifest.json`を1つ配置する。

```text
pcdiag-<日時>-<表示用ID>/
├── collection/
│   └── manifest.json
├── diagnosis/
│   └── manifest.json
└── report/
    └── manifest.json
```

セッション直下には共通の`manifest.json`を配置しない。成果物を後から追加したときにセッション直下のファイルを更新する必要がなくなり、途中停止時の不整合を避けやすくなる。

各成果物は、同一の`session_id`を持つことで同じ診断セッションへ関連付けられる。

## ファイル名と文字コード

- ファイル名は`manifest.json`で固定する。
- 文字コードはUTF-8とする。
- JSONのトップレベルはオブジェクトとする。
- 保存時には正式なスキーマに従って出力する。

## 共通構造

```json
{
  "manifest_schema_version": "1.0",
  "artifact_schema_version": "2.0",
  "session_id": "a3f17c92-d604-4be8-9ea7-6ab7b92e41c5",
  "artifact_id": "831d1074-1145-4a66-bfa2-169903866adb",
  "artifact_type": "collection",
  "status": "partial",
  "started_at": "2026-07-15T01:30:15Z",
  "completed_at": "2026-07-15T01:30:28Z",
  "observed_utc_offset_minutes": 540,
  "duration_ms": 13254,
  "tool": {
    "name": "pcdiag",
    "version": "0.1.0"
  },
  "inputs": [],
  "files": [
    {
      "path": "collection.json",
      "media_type": "application/json",
      "size_bytes": 18234,
      "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
    },
    {
      "path": "status.json",
      "media_type": "application/json",
      "size_bytes": 3210,
      "sha256": "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
    }
  ]
}
```

## スキーマバージョン

スキーマバージョンは`MAJOR.MINOR`形式の10進整数2要素で表す。先頭のゼロ、`v`などの接頭辞、パッチ番号は認めない。

- `MAJOR`は後方互換性のない変更で更新する。
- `MINOR`は後方互換性を維持する変更で更新する。
- 後方互換とは、新しいツールが同一メジャーの古い成果物を読み込めることを指す。古いツールが新しい成果物を読む前方互換は保証しない。
- ツールが`M.N`に対応する場合、`M.0`から`M.N`までを受理する。将来のマイナーおよび異なるメジャーは拒否する。
- 新しく生成する成果物には、入力成果物のバージョンではなく、生成したツールの現在バージョンを記録する。

### `manifest_schema_version`

`manifest.json`自身の構造を表す。

```json
"manifest_schema_version": "1.0"
```

### `artifact_schema_version`

マニフェストが管理する主成果物のデータ形式を表す。

```json
"artifact_schema_version": "2.0"
```

対象例:

- `collection.json`の収集データ形式
- `diagnosis.json`の診断結果形式
- `report.html`のレポート形式

### ツールバージョンとの分離

スキーマバージョンと`pcdiag`のアプリケーションバージョンは別に管理する。

```json
{
  "manifest_schema_version": "1.0",
  "artifact_schema_version": "2.0",
  "tool": {
    "name": "pcdiag",
    "version": "0.1.0"
  }
}
```

`pcdiag`のバージョンが更新されても、保存形式に変更がなければスキーマバージョンは変更しない。

`2.0`では、GPUおよび接続デバイスの`device_state.enabled`を、Windowsの`DN_STARTED`が表す意味に合わせて`device_state.started`へ変更した。この変更には後方互換性がないため、`artifact_schema_version == 1.0`の成果物は`2.0`対応実装への入力として受理しない。

### 更新条件

次はメジャーバージョンを更新する。

- フィールドの削除、名前、型、意味または単位の変更
- nullableから非nullable、任意から必須への変更
- 欠落時の既定値を定義できない必須フィールドの追加
- 既存データを不正にする制約強化
- 列挙値の削除または意味変更

次はマイナーバージョンを更新できる。

- 欠落時の意味を定義した任意フィールドの追加
- 列挙値の追加
- 欠落時の既定値を明確に定義できるフィールドの追加
- nullable化、任意化、許容範囲の拡大などの制約緩和

JSONオブジェクトの未知のフィールドは読み飛ばし、内部モデルへの保持、診断での利用および再出力を保証しない。未知の列挙値、必須フィールドの欠落および型不一致は拒否する。nullableな必須フィールドは明示的な`null`を受理するが、フィールド自体の欠落は拒否する。

### 古い成果物と派生成果物

対応範囲内の古いcollection成果物は現在の診断ルールで再診断できる。再診断は利用者が明示的に実行し、元の成果物を変更せず、現在の`artifact_schema_version`、診断ルールバージョンおよび新しい`artifact_id`を持つ成果物を生成する。診断ルールの変更により、以前の診断結果との一致は保証しない。

collection、diagnosis、reportを組み合わせる場合、入力成果物と派生成果物のartifactメジャーは一致し、入力のマイナーは派生成果物のマイナー以下でなければならない。たとえばcollection `2.0`からdiagnosis `2.1`を生成できるが、collection `2.1`とdiagnosis `2.0`は組み合わせない。collectionとstatusは同じ成果物に含まれ、1つの`artifact_schema_version`を共有する。各manifestの`manifest_schema_version`は個別に対応範囲を検証し、artifact間の完全一致は要求しない。

### サポート範囲とマイグレーション

現在のartifactメジャー内で公開済みのすべてのマイナーバージョンをサポートする。次のメジャーへ移行した後は以前のメジャーをサポートせず、期間保証やLTSは設けない。サポート対象は正式版のpcdiagが生成した未改変のmanifest、collection、statusおよびdiagnosisである。report.htmlは生成物であり、過去のreport.htmlの再読込または変換は保証しない。

メジャー間のマイグレーション機能は提供しない。対応外の成果物は変更せずに拒否し、現行ツールでの再収集または対応する旧ツールの利用を案内する。将来マイグレーションが必要になった場合は別途設計し、元の成果物を上書きしない。

公開済みの各マイナーバージョンについて代表的な固定fixtureを保持し、最新実装で読み込み、診断およびレポート生成ができることをテストする。fixtureは現在の型から再生成せず、新しいマイナーバージョンの公開時に追加する。

## セッションID

`session_id`は、成果物が属する診断セッションの正式な識別子である。

```json
"session_id": "a3f17c92-d604-4be8-9ea7-6ab7b92e41c5"
```

- UUIDv4を使用する。
- 小文字かつハイフンを含む標準UUID文字列とする。
- 同一セッションの`collection`、`diagnosis`、`report`で同じ値を使用する。
- 成果物を組み合わせる際はUUID全体の一致を検証する。

詳細は[セッションID仕様](session-id.md)を参照する。

## 成果物ID

`artifact_id`は、個々の成果物を識別するUUIDv4である。

```json
"artifact_id": "831d1074-1145-4a66-bfa2-169903866adb"
```

同じ診断セッション内で診断やレポートを再生成した場合、それぞれに新しい`artifact_id`を割り当てる。

```text
session_id
├── collection artifact_id A
├── diagnosis artifact_id B
├── diagnosis artifact_id C
└── report artifact_id D
```

- UUIDv4を使用する。
- 小文字かつハイフンを含む標準UUID文字列とする。
- 成果物の生成開始時に1回だけ生成する。
- 一度確定した成果物のIDは変更しない。
- `session_id`と同じ値を使い回さない。

## 成果物種別

`artifact_type`は、マニフェストが管理する成果物の種類を表す。

```json
"artifact_type": "collection"
```

初期仕様で使用する値:

- `collection`
- `diagnosis`
- `report`

成果物ディレクトリと`artifact_type`が一致しない場合は、無効な成果物として扱う。

## 処理状態

`status`は、成果物生成処理の完了状態を表す。

```json
"status": "partial"
```

正式なマニフェストで使用する値:

- `complete`: 必要な成果物を正常に生成した。
- `partial`: 成果物は利用可能だが、一部の処理を完了できなかった。

`status`はPCの健康状態や診断結果の重大度を表すものではない。

例えば、重大な障害が検出された場合でも、すべての診断ルールを正常に実行できていれば診断成果物の状態は`complete`となる。また、情報不足による`not_evaluated`が仕様どおり記録されているだけでは、必ずしも成果物を`partial`としない。

成果物を利用可能な状態まで生成できなかった場合は、正式なマニフェストを確定しない。

```text
complete / partial
    → 正式な成果物ディレクトリへ確定

failed / 強制終了
    → .incompleteディレクトリへログを残す
```

したがって、初期仕様の正式なマニフェストでは`failed`を使用しない。

## 日時

### `started_at`と`completed_at`

成果物生成処理の開始・完了時に、検査対象PCのWindowsシステム時計が報告した日時を記録する。

```json
{
  "started_at": "2026-07-15T01:30:15Z",
  "completed_at": "2026-07-15T01:30:28Z"
}
```

- UTCで保存する。
- RFC 3339形式を使用する。
- UTCを示す`Z`を付ける。
- 日時の正確性は保証しない。
- 成果物の関連付けや一意性の根拠には使用しない。

これらはRTCを直接読み取った値ではなく、Windowsが管理するシステム時計から取得した値である。

### `observed_utc_offset_minutes`

処理開始時に観測したローカル時刻のUTCオフセットを分単位で記録する。

```json
"observed_utc_offset_minutes": 540
```

この例はUTC+09:00を表す。タイムゾーン名や地域名は保存しない。

### `duration_ms`

処理に要した時間をミリ秒単位で記録する。

```json
"duration_ms": 13254
```

- 単調増加時計を使用して測定する。
- Windowsシステム時計やRTCの差分から算出しない。
- 処理中にシステム時刻が変更されても影響を受けないようにする。

## ツール情報

`tool`は、成果物を生成したアプリケーションを記録する。

```json
{
  "tool": {
    "name": "pcdiag",
    "version": "0.1.0"
  }
}
```

初期仕様では以下だけを記録する。

- `name`: 常に`pcdiag`
- `version`: 成果物を生成した`pcdiag`のバージョン

GitコミットID、ビルド日時、Rustのバージョンなどは初期仕様に含めない。

## 入力成果物

`inputs`は、成果物の生成に使用した別の成果物を記録する。

### 収集成果物

`collection`には入力成果物がないため、空配列とする。

```json
"inputs": []
```

### 診断成果物

```json
{
  "inputs": [
    {
      "artifact_id": "831d1074-1145-4a66-bfa2-169903866adb",
      "artifact_type": "collection"
    }
  ]
}
```

### レポート成果物

```json
{
  "inputs": [
    {
      "artifact_id": "831d1074-1145-4a66-bfa2-169903866adb",
      "artifact_type": "collection"
    },
    {
      "artifact_id": "3fb3148a-af52-4472-bf69-cb0dbff41915",
      "artifact_type": "diagnosis"
    }
  ]
}
```

入力元の絶対パスは記録しない。絶対パスにはユーザー名や組織固有の情報が含まれる可能性があるためである。

入力成果物は、次の条件をすべて満たす必要がある。

- `session_id`が現在の成果物と一致する。
- `artifact_id`が`inputs`の値と一致する。
- `artifact_type`が期待する種類と一致する。
- 入力マニフェストと構成ファイルの検証に成功する。

## 構成ファイル一覧

`files`は成果物ディレクトリに含まれる管理対象ファイルを記録する。

```json
{
  "files": [
    {
      "path": "collection.json",
      "media_type": "application/json",
      "size_bytes": 18234,
      "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
    }
  ]
}
```

### `path`

- 成果物ディレクトリを基準とする相対パスとする。
- 区切り文字はWindows上でも`/`を使用する。
- 絶対パスは禁止する。
- `.`および`..`による参照は禁止する。
- 成果物ディレクトリの外部を参照してはならない。

### `media_type`

ファイルのメディアタイプを記録する。

初期仕様で想定する値:

- `application/json`
- `text/plain; charset=utf-8`
- `text/html; charset=utf-8`

### `size_bytes`

ファイルサイズをバイト単位の非負整数で記録する。

### `sha256`

ファイル内容のSHA-256ハッシュ値を、小文字の16進数64文字で記録する。

SHA-256は、USBメモリでの持ち運びやファイルコピー時の破損検出に使用する。暗号化、電子署名、作成者の認証、悪意ある改ざんの防止を保証するものではない。

### 対象外ファイル

`manifest.json`自身は`files`へ含めない。マニフェストが自分自身のハッシュ値を保持すると、循環参照になるためである。

## 確定手順

マニフェストは成果物の完成を示すファイルとして、管理対象ファイルの中で最後に書き込む。

1. `.incomplete`成果物ディレクトリを作成する。
2. 主成果物とログを一時ファイルへ書き込む。
3. 各一時ファイルを確定名へ変更する。
4. ファイルサイズとSHA-256を計算する。
5. `manifest.json`を一時ファイルへ書き込む。
6. `manifest.json`を確定名へ変更する。
7. 成果物ディレクトリから`.incomplete`を除去する。

正式な`manifest.json`が存在し、内容と構成ファイルの検証に成功した場合だけ、完成した成果物として扱う。

各ファイルの書き込み前と手順7の直前に中断要求を確認する。中断要求を検知した場合は手順を打ち切り、`.incomplete`ディレクトリを正式名へ変更しない。書き込み済みのファイルは削除せず、`.incomplete`直下の`interruption.log`へ中断した工程と未完成であることを記録する。`interruption.log`は未完成成果物の調査用ファイルであり、正式なマニフェストの`files`には含めない。

## 検証規則

成果物の読み込み時には、少なくとも次を検証する。

- `manifest_schema_version`が対応範囲内である。
- `artifact_schema_version`が対応範囲内である。
- `session_id`と`artifact_id`が正式なUUIDv4である。
- `artifact_type`が既知の値である。
- `status`が`complete`または`partial`である。
- 日時が指定形式である。
- `duration_ms`と`size_bytes`が非負整数である。
- `path`が安全な相対パスである。
- 各ファイルが存在する。
- 各ファイルのサイズが一致する。
- 各ファイルのSHA-256が一致する。
- 入力成果物のIDと種類が一致する。
- 関連するすべての成果物で`session_id`が一致する。

検証に失敗した成果物は後続処理へ渡さず、入力エラーとして扱う。

## 機密情報の制約

共通マニフェストには、次の情報を保存しない。

- コンピューター名
- ユーザー名
- メールアドレス
- IPアドレスまたはMACアドレス
- ファイルの絶対パス
- Windowsのプロダクトキー
- PCやデバイスのシリアル番号
- ハードウェア固有ID
- 時刻同期先のホスト名またはIPアドレス
- 収集した診断データそのもの

マニフェストは成果物管理に必要な情報だけを保持する。

## 決定事項

| 項目 | 決定内容 |
|---|---|
| 配置 | 成果物ディレクトリごとに`manifest.json`を配置 |
| セッション直下のマニフェスト | 初期実装では作成しない |
| セッションID | UUIDv4 |
| 成果物ID | 成果物ごとにUUIDv4を生成 |
| 成果物種別 | `collection`、`diagnosis`、`report` |
| 正式な処理状態 | `complete`、`partial` |
| 失敗・中断時 | `.incomplete`にログを残し、正式成果物にしない。確定済み成果物は保持する |
| 日時 | Windowsシステム時計から取得しUTCのRFC 3339形式で保存 |
| 処理時間 | 単調増加時計で測定しミリ秒で保存 |
| 入力関係 | `artifact_id`と`artifact_type`で記録 |
| パス | 成果物ディレクトリ基準の相対パス |
| ファイルサイズ | バイト単位で記録 |
| ハッシュ | SHA-256を小文字16進数で記録 |
| 外部通信 | 不要 |
| 検査対象PCの時計情報 | マニフェストには含めず`collection.json`で扱う |
