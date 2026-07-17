# 診断データモデル仕様

## 目的

`collection.json`、`status.json`、`diagnosis.json`を一体の診断データ仕様として設計する。

3ファイルは個別に完結するものではなく、次の責務を分担する。

```text
collection.json
    客観的な収集値
          │
          ├──────────────┐
          ↓              ↓
status.json         diagnosis.json
取得状態と理由       収集値に対する評価
```

## 設計原則

- ファイル単位ではなく検査カテゴリ単位で3ファイルを同時に設計する。
- `collection.json`には観測した事実だけを保存する。
- `status.json`には収集処理の状態と取得不能理由を保存する。
- `diagnosis.json`には診断ルールによる評価と根拠を保存する。
- 診断処理は収集成果物を書き換えない。
- 取得不能を正常値として評価しない。
- 診断根拠から元の収集値を追跡できるようにする。
- Issue #2「CLI基本仕様」の収集・診断・レポート要件を網羅する。
- 機密情報として禁止されたフィールドを初期スキーマへ含めない。

## メタデータの正本

以下の成果物管理情報は各成果物の`manifest.json`だけに保存し、データファイルへ重複させない。

- `session_id`
- `artifact_id`
- `artifact_type`
- マニフェストスキーマバージョン
- 成果物スキーマバージョン
- 作成開始・完了日時
- 処理時間
- `pcdiag`のバージョン
- 入力成果物
- 構成ファイル一覧とSHA-256

JSONファイル単体ではなく、`manifest.json`を含む成果物ディレクトリを受け渡し単位とする。

詳細は[共通マニフェスト仕様](common-manifest.md)を参照する。

## collection.json

### 責務

診断対象PCから収集し、共通形式へ正規化した客観的な情報を保存する。故障判定、重大度、推奨事項は保存しない。

### ルート構造

```json
{
  "windows": {},
  "clock": {},
  "cpu": {
    "packages": []
  },
  "memory": {},
  "storage": {
    "disks": [],
    "volumes": []
  },
  "firmware": {},
  "devices": [],
  "gpus": []
}
```

初期スキーマで定義されたカテゴリは、収集に失敗した場合も省略しない。オブジェクト内の値を`null`とし、理由を`status.json`へ記録する。

配列は正常に列挙した結果が0件なら空配列とする。GPUの列挙自体に失敗または未実行の場合は`gpus`を`null`とし、理由を`status.json`へ記録する。

## status.json

### 責務

各コレクターの実行状態、処理時間、警告、エラー、フィールド単位の取得不能理由を保存する。

正常に取得した個々のフィールドは記録せず、例外だけを記録する。

### ルート構造

```json
{
  "collectors": []
}
```

### コレクター構造

```json
{
  "name": "gpu",
  "status": "partial",
  "duration_ms": 84,
  "messages": [],
  "fields": [
    {
      "path": "/gpus/0/temperature_celsius",
      "status": "unsupported",
      "code": "gpu_temperature_not_exposed"
    }
  ]
}
```

コレクター状態:

- `success`
- `partial`
- `skipped`
- `failed`

フィールド取得状態:

- `source_null`
- `unsupported`
- `not_applicable`
- `permission_denied`
- `timeout`
- `failed`
- `not_collected`
- `invalid_value`

## diagnosis.json

### 責務

収集値と取得状態へ診断ルールを適用した結果を保存する。問題を検出したルールだけでなく、実行対象となった全ルールの評価結果を保存する。

### ルート構造

```json
{
  "rule_set": {
    "name": "pcdiag_builtin",
    "version": "0.1.0"
  },
  "summary": {
    "overall_severity": "warning",
    "evaluations": {
      "passed": 4,
      "triggered": 1,
      "not_applicable": 0,
      "not_evaluated": 1,
      "failed": 0
    },
    "findings": {
      "critical": 0,
      "error": 0,
      "warning": 1,
      "information": 0
    }
  },
  "evaluations": []
}
```

### 評価状態

- `passed`: 問題を検出しなかった。
- `triggered`: 問題または注意事項を検出した。
- `not_applicable`: 評価対象が存在しない。
- `not_evaluated`: 情報不足により評価できない。
- `failed`: 診断ルール自体の処理に失敗した。

### 重大度

重大度の具体的な体系は別途確定する。初期構造では次の値を候補とする。

- `critical`
- `error`
- `warning`
- `information`

`passed`、`not_applicable`、`not_evaluated`では`severity`を`null`にできる。

## ファイル間の不変条件

### セッションの一致

収集成果物と診断成果物の`manifest.json`に記録された`session_id`が一致しなければならない。

### nullの説明

`collection.json`の現在のスキーマで定義されたフィールドが`null`の場合、次のいずれかが必要である。

1. `status.json`に同じフィールドを指すJSON Pointerと取得不能理由がある。
2. 対応コレクターが`skipped`または`failed`であり、コレクター単位の理由がある。

### 診断根拠の存在

`diagnosis.json`の収集根拠が参照するJSON Pointerは、入力した`collection.json`に存在しなければならない。

### 収集根拠の値

収集根拠へ値を複写する場合、その値はJSON Pointerで参照した`collection.json`の値と一致しなければならない。

### 評価不能の説明

収集情報の不足による`not_evaluated`は、必要な収集パスと`status.json`の取得状態を追跡できなければならない。

### 不変性

成果物確定後は`collection.json`、`status.json`、`diagnosis.json`を書き換えない。再収集または再診断では、新しい成果物IDを持つ成果物を生成する。

## メモリカテゴリ

メモリを最初の縦断モデルとして、3ファイル間の関係を定義する。

### collection.json

```json
{
  "memory": {
    "physical": {
      "total_bytes": 17179869184,
      "available_bytes": 536870912,
      "load_percent": 97.0
    },
    "commit": {
      "limit_bytes": 25769803776,
      "available_bytes": 9126805504
    },
    "virtual": {
      "total_bytes": 140737488224256,
      "available_bytes": 140732881338368
    }
  }
}
```

#### 物理メモリ

- `total_bytes`: Windowsが利用可能と認識している物理メモリ総量
- `available_bytes`: 直ちに利用できる物理メモリ量
- `load_percent`: Windowsが収集時に報告したメモリ負荷率

#### コミット

- `limit_bytes`: コミット可能な上限
- `available_bytes`: 残りのコミット可能容量

#### 仮想アドレス空間

- `total_bytes`: 利用可能な仮想アドレス空間の総量
- `available_bytes`: 未使用の仮想アドレス空間

### 派生値

元の値から一意に算出できる値は、原則として`collection.json`へ重複保存しない。

```text
physical.used_bytes
    = physical.total_bytes - physical.available_bytes

physical.available_percent
    = physical.available_bytes / physical.total_bytes × 100
```

Windowsが直接報告した`load_percent`は観測値として保存する。`pcdiag`が計算した値は診断根拠の派生値として扱う。

### 収集成功時のstatus.json

```json
{
  "collectors": [
    {
      "name": "memory",
      "status": "success",
      "duration_ms": 12,
      "messages": [],
      "fields": []
    }
  ]
}
```

### 部分的な取得不能

`collection.json`:

```json
{
  "memory": {
    "physical": {
      "total_bytes": 17179869184,
      "available_bytes": 536870912,
      "load_percent": 97.0
    },
    "commit": {
      "limit_bytes": null,
      "available_bytes": null
    },
    "virtual": {
      "total_bytes": null,
      "available_bytes": null
    }
  }
}
```

`status.json`:

```json
{
  "collectors": [
    {
      "name": "memory",
      "status": "partial",
      "duration_ms": 12,
      "messages": [],
      "fields": [
        {
          "path": "/memory/commit/limit_bytes",
          "status": "source_null",
          "code": "source_returned_null"
        },
        {
          "path": "/memory/commit/available_bytes",
          "status": "source_null",
          "code": "source_returned_null"
        },
        {
          "path": "/memory/virtual/total_bytes",
          "status": "unsupported",
          "code": "virtual_memory_information_unavailable"
        },
        {
          "path": "/memory/virtual/available_bytes",
          "status": "unsupported",
          "code": "virtual_memory_information_unavailable"
        }
      ]
    }
  ]
}
```

### コレクター全体の失敗

メモリ情報取得API全体が失敗した場合、`collection.json`のメモリ値をすべて`null`とし、コレクター単位の状態と理由で代表する。

```json
{
  "name": "memory",
  "status": "failed",
  "duration_ms": 3,
  "messages": [
    {
      "code": "windows_api_failed",
      "native_code": 5,
      "message": "メモリ情報を取得できませんでした"
    }
  ],
  "fields": []
}
```

### 診断結果

以下の閾値は構造を説明する例であり、正式な診断閾値ではない。

```json
{
  "rule_id": "memory.available_ratio",
  "rule_version": "1.0",
  "category": "memory",
  "status": "triggered",
  "severity": "warning",
  "summary": "使用可能な物理メモリが少なくなっています",
  "evidence": [
    {
      "kind": "collected",
      "path": "/memory/physical/total_bytes",
      "value": 17179869184
    },
    {
      "kind": "collected",
      "path": "/memory/physical/available_bytes",
      "value": 536870912
    },
    {
      "kind": "derived",
      "name": "available_percent",
      "value": 3.125,
      "unit": "percent",
      "source_paths": [
        "/memory/physical/total_bytes",
        "/memory/physical/available_bytes"
      ]
    }
  ],
  "criterion": {
    "operator": "less_than",
    "threshold": 10.0,
    "unit": "percent"
  },
  "recommendation": {
    "code": "review_memory_consumption"
  }
}
```

### 評価不能

```json
{
  "rule_id": "memory.available_ratio",
  "rule_version": "1.0",
  "category": "memory",
  "status": "not_evaluated",
  "severity": null,
  "summary": "物理メモリの利用可能割合を評価できませんでした",
  "evidence": [],
  "reason": {
    "code": "required_collection_value_unavailable",
    "paths": [
      "/memory/physical/total_bytes",
      "/memory/physical/available_bytes"
    ]
  },
  "recommendation": null
}
```

## 診断根拠

診断根拠には、参照パスと評価時の値を記録する。

### 収集値

```json
{
  "kind": "collected",
  "path": "/memory/physical/available_bytes",
  "value": 536870912
}
```

### 派生値

```json
{
  "kind": "derived",
  "name": "available_percent",
  "value": 3.125,
  "unit": "percent",
  "source_paths": [
    "/memory/physical/total_bytes",
    "/memory/physical/available_bytes"
  ]
}
```

収集値を診断結果へ複写することで、診断時に使用した値を明確にする。読み込み時には、`kind`が`collected`の根拠について、`path`で解決した収集値と`value`が一致することを検証する。

## 今後のカテゴリ設計順序

1. GPU
2. ストレージ
3. 接続デバイス
4. Windowsと時計
5. CPU
6. BIOS・UEFI

GPUでは、複数要素、配列参照、取得不能テレメトリー、ドライバー情報を使ってモデルを検証する。

## GPUカテゴリ

### 初期実装の対象

- 複数GPUの列挙
- GPU名とメーカー名
- アダプター種別（ハードウェア、ソフトウェア、リモート、不明）
- WindowsのデバイスインスタンスID
- PCIベンダーID、デバイスID、サブシステムID、リビジョンID
- 専用ビデオメモリ、専用システムメモリ、共有システムメモリ
- ドライバーのバージョンと日付
- 現在の存在、無効化状態、デバイス問題コード

デバイスインスタンスIDはGPUの照合に必要な技術情報として収集する。ただし、収集バンドルを外部へ提供する前に確認すべき情報として扱う。

### 初期実装の対象外

- 温度、使用率、消費電力、クロック速度、ファン速度
- GPUメーカー固有APIによる情報
- ディスプレイおよび映像出力端子との接続関係
- 診断閾値とGPU性能の評価

これらはWindowsの標準的な情報源だけではGPU間で一貫して取得できないため、初期実装へ含めない。取得不能な項目として`null`を並べるのではなく、スキーマ自体へ定義しない。

### collection.json

```json
{
  "gpus": [
    {
      "name": "Example GPU",
      "vendor": "Example Vendor",
      "adapter_type": "hardware",
      "device_instance_id": "PCI\\VEN_1234&DEV_5678&SUBSYS_00000000&REV_01\\...",
      "pci": {
        "vendor_id": 4660,
        "device_id": 22136,
        "subsystem_id": 0,
        "revision_id": 1
      },
      "memory": {
        "dedicated_video_bytes": 8589934592,
        "dedicated_system_bytes": 0,
        "shared_system_bytes": 34210639872
      },
      "driver": {
        "version": "1.2.3.4",
        "date": "2026-07-15"
      },
      "device_state": {
        "present": true,
        "enabled": true,
        "problem_code": 0
      }
    }
  ]
}
```

`gpus`の意味:

- 1件以上: 列挙に成功し、検出したGPUを保存した。
- 空配列: 列挙に成功したがGPUは検出されなかった。
- `null`: 列挙に失敗した、対応していない、権限不足、または収集を実行しなかった。

各GPUオブジェクトのキーは省略しない。取得できない値は`null`とし、対応するJSON Pointerと理由を`status.json`へ記録する。

`adapter_type`の値:

- `hardware`: 物理GPUアダプター
- `software`: Microsoft Basic Render Driverなどのソフトウェアアダプター
- `remote`: リモート表示用アダプター
- `unknown`: 情報源から種別を判定できないアダプター

ソフトウェアおよびリモートアダプターも収集対象から除外しない。レポートで物理GPUと区別して表示し、診断時の状況判断に利用できるようにする。

PCI識別子はJSON上では数値で保存する。画面やHTMLレポートでは、必要に応じて`10DE`のような4桁の16進表記へ変換する。

`driver.date`は時刻を含まない日付として`YYYY-MM-DD`形式で保存する。情報源が有効な日付を返さない場合は`null`とする。

Windows実装では、DXGIのAdapter LUIDとSetupAPIの`DEVPKEY_Device_AdapterLuid`を最優先で照合して同一アダプターを特定する。

Adapter LUIDを取得できない環境では、デバイスインスタンスIDから`VEN`、`DEV`、`SUBSYS`、`REV`を解析し、DXGIのPCI識別子と完全一致する候補が1件だけの場合に限って結合する。同じ識別子を持つ候補が複数存在する場合は、誤った情報を結合せず、対象フィールドを`null`とする。

デバイスインスタンスID、ドライバーバージョン、ドライバー日付、デバイス開始状態、問題コードは、対応するSetupAPIデバイスプロパティから取得する。DXGIアダプターに対応するデバイス情報が見つからない場合は、対象フィールドを`null`とし、`status.json`へ理由を記録する。

### status.json

GPUの列挙に成功し、一部フィールドだけ取得できなかった例:

```json
{
  "name": "gpu",
  "status": "partial",
  "duration_ms": 18,
  "messages": [],
  "fields": [
    {
      "path": "/gpus/0/memory/dedicated_video_bytes",
      "status": "unsupported",
      "code": "dedicated_video_memory_unavailable"
    }
  ]
}
```

列挙自体に失敗した場合は`gpus`を`null`とし、コレクター単位の理由を記録する。

```json
{
  "name": "gpu",
  "status": "failed",
  "duration_ms": 4,
  "messages": [
    {
      "code": "gpu_enumeration_failed",
      "native_code": 5,
      "message": "GPUを列挙できませんでした"
    }
  ],
  "fields": []
}
```

### diagnosis.json

初期段階ではGPU情報の収集と表示を優先する。診断規則は、取得情報とWindows実機での差異を確認してから定義する。

最初の診断候補は次のとおりとする。

- Windowsがデバイス問題コードを報告している。
- GPUは存在するが無効化されている。
- ドライバー情報を取得できない。
- 同一のデバイスインスタンスIDが重複している。

## 未決定事項

- 診断結果の正式な重大度体系
- 診断ルールIDの命名規則
- 診断ルールセットのバージョン規則
- 診断根拠の`value`が取り得るJSON型
- 診断条件の演算子一覧
- 配列要素へのJSON Pointerを長期的に安定させる方法
- メモリモジュール単位の情報を初期実装へ含めるか
