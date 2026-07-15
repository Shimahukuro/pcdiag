# 共通データ表現仕様

## 目的

`pcdiag`が生成するJSONにおいて、日時、処理時間、容量、割合、測定値、真偽値、状態、取得不能値などを一貫した形式で表現する。

本仕様は、主に次の成果物へ適用する。

- `collection.json`
- `status.json`
- `diagnosis.json`
- 各成果物の`manifest.json`

成果物固有の構造は別の仕様で定義し、本書では複数の成果物で共有する表現と不変条件を定義する。

## 基本方針

- JSONはUTF-8で保存する。
- フィールド名と列挙値は小文字の`snake_case`へ統一する。
- 数値の単位はフィールド名で明示する。
- 保存値と人向けの表示値を分離する。
- 取得できなかった値を`0`、空文字列、`false`などで代用しない。
- `null`だけで取得状態を表現せず、`status.json`へ理由を記録する。
- 値が存在しない状態と、取得処理が失敗した状態を区別する。
- JSONでは`NaN`および正負の無限大を使用しない。
- 配列の順序へ意味を持たせない。

## フィールド名

フィールド名には小文字の`snake_case`を使用する。

```json
{
  "total_bytes": 17179869184,
  "duration_ms": 13254,
  "temperature_celsius": 62.5
}
```

単位を持つ値は、可能な限りフィールド名の末尾へ単位を付ける。

```text
*_bytes
*_ms
*_percent
*_celsius
*_hz
*_rpm
*_watts
*_volts
```

単位をフィールド外の暗黙的な仕様だけに依存させない。

## 日時

日時はUTCのRFC 3339文字列として保存する。

```json
{
  "observed_at": "2026-07-15T01:30:15.123Z"
}
```

規則:

- UTCで保存する。
- UTCを示す`Z`を付ける。
- 秒未満が必要な場合はミリ秒まで記録できる。
- ローカル表示への変換はレポート生成時に行う。
- 日時の正確性は保証しない。
- 日時を成果物の一意性や関連付けの根拠にしない。

実行時に観測したUTCオフセットを保存する場合は、分単位の符号付き整数を使用する。

```json
{
  "observed_utc_offset_minutes": 540
}
```

UTC+09:00は`540`、UTC-05:00は`-300`となる。タイムゾーン名や地域名は、共通表現としては保存しない。

マニフェストに記録する日時の詳細は[共通マニフェスト仕様](common-manifest.md)を参照する。

検査対象PCのシステム時計、Windows Timeサービス、時刻同期状態、ハードウェアRTCなどは診断対象の情報であり、`collection.json`の時計カテゴリで扱う。

## 処理時間

処理時間はミリ秒単位の非負整数として保存する。

```json
{
  "duration_ms": 13254
}
```

規則:

- 単調増加時計を使用して測定する。
- システム時計やRTCの差分から算出しない。
- 処理中の時刻同期や時刻変更の影響を受けないようにする。
- 1ミリ秒未満の処理は`0`として表現できる。
- 秒、マイクロ秒、ナノ秒を同じフィールドで混在させない。

## 容量

容量はバイト単位の非負整数として保存する。

```json
{
  "total_bytes": 1000204886016,
  "available_bytes": 536870912000
}
```

JSONへ`GB`や`GiB`へ変換した表示値を保存しない。表示時にレポート側で変換する。

人向け表示ではIEC単位を基本とする。

| 単位 | バイト数 |
|---|---:|
| KiB | 1024 |
| MiB | 1024² |
| GiB | 1024³ |
| TiB | 1024⁴ |

ストレージメーカーの公称容量との比較が必要な場合は、SI単位を併記できる。ただし、保存されているバイト数を変更しない。

## 個数

CPUコア数やデバイス数などは非負整数として保存する。

```json
{
  "physical_cores": 8,
  "logical_processors": 16,
  "device_count": 24
}
```

取得できなかった個数を`0`で表現しない。`0`は、正常に確認した結果として対象が0件または0個だった場合にのみ使用する。

## 割合

割合は`0.0`から`100.0`までの有限なJSON数値として保存する。

```json
{
  "memory_usage_percent": 68.25,
  "free_space_percent": 12.5
}
```

規則:

- フィールド名へ`_percent`を付ける。
- `%`記号を含む文字列として保存しない。
- 0から100の範囲とする。
- `NaN`や無限大を保存しない。
- 計算不能の場合は`null`とし、理由を`status.json`へ記録する。
- 保存時に必要以上の丸めを行わない。
- 表示時の小数桁はレポート側で整える。

## 測定値と単位

初期仕様では、次の基準単位を使用する。

| 値 | 保存単位 | フィールド例 |
|---|---|---|
| 温度 | 摂氏 | `temperature_celsius` |
| 周波数 | Hz | `clock_hz` |
| 回転数 | RPM | `fan_speed_rpm` |
| 電力 | W | `power_watts` |
| 電圧 | V | `voltage_volts` |
| 使用率 | % | `utilization_percent` |
| 処理時間 | ms | `duration_ms` |
| 容量 | byte | `capacity_bytes` |

小数が必要な測定値は有限なJSON数値として保存する。

```json
{
  "temperature_celsius": 62.5,
  "power_watts": 87.25
}
```

## 真偽値

真偽値はJSONの`true`または`false`を使用する。

```json
{
  "secure_boot_enabled": true,
  "driver_signed": false
}
```

`yes`、`no`、`enabled`、`disabled`などの文字列を真偽値の代わりに使用しない。

値を確定できなかった場合は`null`とし、理由を`status.json`へ記録する。

```json
{
  "secure_boot_enabled": null
}
```

## 文字列

文字列はUTF-8として保存する。

正規化規則:

- 前後の不要な空白を除去する。
- 末尾のNUL文字を除去する。
- 必要に応じて改行コードをLFへ正規化する。
- メーカー名やモデル名の大文字・小文字は原則として保持する。
- 取得不能値を空文字列で表現しない。
- `Unknown`、`N/A`、`-`などを独自の不明値として保存しない。

```json
{
  "manufacturer": "Example Vendor",
  "model": null
}
```

データソースが文字列として`Unknown`などを返した場合に、それが実データかデータソース固有の欠損表現かを正規化処理で判定する。欠損表現と判断した場合は`null`へ変換し、`status.json`へ`source_null`または適切な理由を記録する。

## 列挙値

状態、種別、理由などの列挙値には、小文字の`snake_case`文字列を使用する。

```json
{
  "presence": "not_present",
  "device_status": "degraded",
  "collector_status": "partial"
}
```

`OK`、`Ok`、`ok`などの表記揺れを許容しない。保存時には正式な列挙値へ正規化する。

## 状態型

異なる用途の状態を一つの共通列挙型へ統合しない。用途ごとに独立した型を定義する。

### 成果物状態

```text
ArtifactStatus
├── complete
└── partial
```

### コレクター状態

```text
CollectorStatus
├── success
├── partial
├── skipped
└── failed
```

### 診断ルール評価状態

```text
RuleEvaluationStatus
├── passed
├── triggered
├── not_applicable
├── not_evaluated
└── failed
```

### デバイス存在状態

```text
DevicePresence
├── present
├── not_present
└── unknown
```

別用途の状態型を分けることで、Rustの型によって無効な組み合わせを防止する。

## 値の存在状態

JSON内の表現を次のように定義する。

| JSON表現 | 意味 |
|---|---|
| 値あり | 値を正常に取得・正規化できた |
| `null` | 現在のスキーマに存在する項目だが値を確定できなかった |
| 空配列`[]` | 列挙に成功し、対象が0件だった |
| フィールドなし | そのスキーマバージョンでは項目が定義されていない |

現在のスキーマで定義されているフィールドは、値を取得できない場合も省略せず`null`として出力する。

取得不能値を`0`、`false`、空文字列、空配列で代用してはならない。

## `null`と取得状態

`null`だけでは、値を確定できなかった理由を判別できない。

例えば、次のJSONだけでは、データソースがNULLを返したのか、権限不足やAPIエラーが発生したのかを判断できない。

```json
{
  "temperature_celsius": null
}
```

そのため、`collection.json`と`status.json`を一組として扱う。

```text
collection.json
    → 収集値、またはnull

status.json
    → nullになった具体的な理由
```

### 不変条件

> `collection.json`の現在のスキーマで定義されたフィールドが`null`の場合、`status.json`にはそのフィールドを直接指すJSON Pointerと理由が存在するか、対応するコレクター全体が`skipped`または`failed`であり、コレクター単位の理由が存在しなければならない。

部分的な取得不能はフィールド単位で記録する。コレクター全体の呼び出しが失敗し、配下のすべての値を取得できなかった場合は、同じ理由を各フィールドへ繰り返さず、コレクター単位の状態とメッセージで代表できる。

いずれの説明もない`null`を含む収集成果物は、整合性のない成果物として扱う。

## フィールド取得状態

フィールド単位の取得状態は次のとおりとする。

| 状態 | 意味 |
|---|---|
| `source_null` | APIやデータソースの呼び出しには成功したが、値としてNULLまたは欠損値が返された |
| `unsupported` | OS、API、ドライバー、ハードウェアが取得機能に対応していない |
| `not_applicable` | そのPCまたはデバイスには項目自体が適用されない |
| `permission_denied` | 権限不足により取得できなかった |
| `timeout` | 制限時間内に取得できなかった |
| `failed` | 取得処理がエラーで失敗した |
| `not_collected` | 実行モードやオプションによって収集しなかった |
| `invalid_value` | 値は返されたが、妥当な値として解釈できなかった |

原因を特定できる場合は、汎用的な`unavailable`ではなく具体的な状態を使用する。

### `source_null`

データソースの呼び出しが成功し、値としてNULLが返された状態である。

```json
{
  "path": "/gpus/0/fan_speed_rpm",
  "status": "source_null",
  "code": "source_returned_null"
}
```

### `unsupported`

項目は適用される可能性があるが、現在のハードウェア、ドライバー、OS、APIでは取得できない状態である。

```json
{
  "path": "/gpus/0/temperature_celsius",
  "status": "unsupported",
  "code": "driver_telemetry_not_supported"
}
```

### `not_applicable`

項目の概念自体が対象へ適用されない状態である。

```json
{
  "path": "/gpus/0/fan_speed_rpm",
  "status": "not_applicable",
  "code": "passively_cooled_gpu"
}
```

### `failed`

データソースの呼び出しや値の変換がエラーで失敗した状態である。

```json
{
  "path": "/gpus/0/fan_speed_rpm",
  "status": "failed",
  "code": "windows_api_failed",
  "native_code": 5
}
```

### `not_collected`

現在のスキーマには存在するが、選択された実行モードやオプションによって収集しなかった場合に使用する。

機密情報除外方針によって常に収集しないシリアル番号などは、フィールド自体を初期スキーマへ含めないため、`not_collected`としても出力しない。

## status.jsonからのフィールド参照

`status.json`から`collection.json`のフィールドを指すため、`path`にはJSON Pointer形式を使用する。

```text
/gpus/0/temperature_celsius
/storage/disks/1/health
/windows/secure_boot_enabled
```

例:

```json
{
  "collectors": [
    {
      "name": "gpu",
      "status": "partial",
      "fields": [
        {
          "path": "/gpus/0/temperature_celsius",
          "status": "unsupported",
          "code": "gpu_temperature_not_exposed"
        },
        {
          "path": "/gpus/0/fan_speed_rpm",
          "status": "source_null",
          "code": "source_returned_null"
        }
      ]
    }
  ]
}
```

JSON Pointerは対象となる`collection.json`の確定後の構造を参照する。成果物確定後に配列の並びやフィールド構造を変更してはならない。

## コレクター状態の集計

フィールド単位の取得状態から、コレクター全体の状態を集計する。

```text
必要な項目をすべて取得
    → success

一部を取得できなかったが利用可能な結果がある
    → partial

権限不足や非対応によりコレクターを実行しなかった
    → skipped

コレクター自体が失敗し、利用可能な結果がない
    → failed
```

`not_applicable`は失敗ではない。例えば、バッテリーを正常に列挙した結果、搭載されていなかった場合は、コレクターを`success`として扱える。

```json
{
  "name": "battery",
  "status": "success",
  "result": "not_applicable"
}
```

## エラーコード

機械処理用の安定したコードと、人が読む説明を分離する。

```json
{
  "code": "permission_denied",
  "message": "管理者権限がないため情報を取得できませんでした"
}
```

規則:

- `code`は英語の`snake_case`とする。
- 診断処理やプログラム分岐には`code`を使用する。
- `message`の文面へ処理を依存させない。
- `message`は機密情報を含まない。
- Windows APIなどの生のエラーコードは`native_code`へ分離する。

```json
{
  "code": "windows_api_failed",
  "native_code": 5,
  "message": "情報へのアクセスが拒否されました"
}
```

## 配列

配列の順序はデータの意味として使用しない。

```json
{
  "gpus": [],
  "disks": [],
  "devices": []
}
```

Windows APIの列挙順は実行ごとに変わる可能性がある。保存時には可能な範囲で安定した並びへ整えるが、読み込み側は順序に依存してはならない。

ただし、`status.json`のJSON Pointerは確定済みの`collection.json`内の配列位置を参照するため、成果物確定後に配列順序を変更しない。

## 診断処理での扱い

`diagnose`は`collection.json`と`status.json`の両方を読み込む。

```text
値あり
    → 診断ルールを実行

null + not_applicable
    → not_applicable

null + unsupported
    → not_evaluated

null + permission_denied
    → not_evaluated
    → 必要に応じて権限に関する案内を生成

null + failed
    → not_evaluated
    → 収集エラーを記録

null + status記録なし
    → 不正または不整合な収集成果物
```

取得できなかった情報を正常値として評価してはならない。

## 採用しない表現

初期仕様では、各値を状態オブジェクトで包む形式を採用しない。

```json
{
  "temperature_celsius": {
    "status": "unsupported",
    "value": null
  }
}
```

この形式は単独で意味が完結する一方、すべての正常値もオブジェクトで包む必要があり、`collection.json`、Rust型、レポート生成が冗長になる。

代わりに、値と取得状態を次の2ファイルへ分離する。

```text
collection.json
    → 正規化された値

status.json
    → 取得処理の状態と理由
```

## 決定事項

| 項目 | 決定内容 |
|---|---|
| JSON文字コード | UTF-8 |
| フィールド名 | 小文字の`snake_case` |
| 日時 | UTCのRFC 3339文字列 |
| UTCオフセット | 分単位の符号付き整数 |
| 処理時間 | 単調増加時計で測定したミリ秒 |
| 容量 | バイト単位の非負整数 |
| 人向け容量表示 | IEC単位を基本とする |
| 割合 | 0から100の有限なJSON数値 |
| 温度 | 摂氏 |
| 周波数 | Hz |
| 真偽値 | `true`、`false`、取得不能時は`null` |
| 不明・取得不能値 | `null`と`status.json`の理由を組み合わせる |
| 正常に0件 | 空配列 |
| 現在スキーマのフィールド | 値または`null`を必ず出力 |
| 列挙値 | 小文字の`snake_case` |
| 状態型 | 用途ごとに独立した列挙型 |
| エラーコード | 安定した英語の`snake_case`コード |
| フィールド参照 | JSON Pointer |
| 配列順序 | 意味を持たせない |
| 値オブジェクト形式 | 初期仕様では採用しない |

## 今後の検討事項

- JSON数値として安全に扱える整数範囲と、非常に大きい整数の表現
- 測定値の精度と丸め規則
- 未知の列挙値を読み込んだ場合の互換性方針
- JSON Pointerで配列要素を参照する際の長期的な安定性
- フィールド単位の取得時刻が必要な測定項目
- エラーコード体系の具体的な一覧
