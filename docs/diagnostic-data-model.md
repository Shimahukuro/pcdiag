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

`not_applicable`は取得失敗を意味しない。適用可能な項目をすべて取得できている場合、`fields`に`not_applicable`が含まれていてもコレクター状態を`success`にできる。

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

初期実装では`memory.available_ratio`を正式な組み込み診断ルールとして使用する。物理メモリ総容量に対する利用可能容量の割合が10%未満の場合、`warning`を発生させる。

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

`memory.available_ratio`の判定規則:

- ルールID: `memory.available_ratio`
- ルールバージョン: `1.0`
- 演算子: `less_than`
- 閾値: `10.0 percent`
- 検出時の重大度: `warning`
- 推奨コード: `review_memory_consumption`
- 10%以上の場合: `passed`
- 必要な収集値が`null`の場合: `not_evaluated`

診断ルールセット名は`pcdiag_builtin`とする。メモリ規則のみの初期バージョンを`0.1.0`、GPU規則を追加したバージョンを`0.2.0`、正常時を含むGPU診断根拠を追加したバージョンを`0.2.1`、GPUデバイスインスタンスIDの重複検出を追加したバージョンを`0.3.0`、接続デバイス診断を追加したバージョンを`0.4.0`、Windows実機結果に基づいて開始状態と有効状態を区別し、接続デバイス診断を問題コードへ限定したバージョンを`0.5.0`、SMARTとボリューム空き容量の診断を追加した現在のバージョンを`0.6.0`とする。診断成果物は収集成果物のマニフェストとファイル完全性を検証した後にだけ生成する。

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

## カテゴリ実装状況

CPU、BIOS・UEFI、GPU、ストレージ、接続デバイス、Windows基本情報、時計情報は実装済みである。

## Windows基本情報カテゴリ

Windows基本情報は`windows`へ保存し、コレクター名は`windows`とする。

```json
{
  "windows": {
    "edition": "Professional",
    "version": "10.0.26100",
    "build_number": 26100,
    "architecture": "x86_64",
    "booted_at": "2026-07-17T00:00:00.000Z",
    "uptime_ms": 123000,
    "boot_mode": "uefi"
  }
}
```

- `edition`: `GetProductInfo`が報告した製品種別を表示名へ変換する。既知でない製品種別は`product_<番号>`として情報を失わず保存する。
- `version`: メジャー、マイナー、ビルド番号を`<major>.<minor>.<build>`形式で保存する。
- `build_number`: Windowsのビルド番号を数値で保存する。
- `architecture`: `x86`、`x86_64`、`arm`、`arm64`、`unknown`のいずれかとする。
- `booted_at`: 収集時のWindowsシステム時刻から稼働時間を引いて算出し、UTCのRFC 3339形式で保存する。
- `uptime_ms`: Windows起動後の経過時間をミリ秒で保存する。
- `boot_mode`: `bios`、`uefi`、`unknown`のいずれかとする。

Windows実装では、マニフェストの影響を受けず実際のOSバージョンを取得するため`RtlGetVersion`を使用する。システムアーキテクチャは`GetNativeSystemInfo`、稼働時間は`GetTickCount64`、起動方式は`GetFirmwareType`を使用する。

取得できない値は`null`とし、対応する`/windows/...`のJSON Pointerと理由を`status.json`へ記録する。一部だけ取得できない場合も取得済み情報を保持してコレクターを`partial`とする。

`booted_at`はWindowsのシステム時刻を基準にした算出値であり、ハードウェアRTCが保持する時刻ではない。システム時刻、UTCオフセット、Windows Timeサービス、ハードウェアRTCは、後続の時計情報カテゴリで別に収集する。

## 時計情報カテゴリ

時計情報は`clock`へ保存し、コレクター名は`clock`とする。

```json
{
  "clock": {
    "system_time_utc": "2026-07-17T04:00:00.000Z",
    "utc_offset_minutes": 540,
    "windows_time_service": "running",
    "hardware_clock": null
  }
}
```

- `system_time_utc`: Windowsのシステム時計が報告した時刻をUTCのRFC 3339形式で保存する。
- `utc_offset_minutes`: 収集時点で有効な、UTCからローカル時刻へのオフセットを分単位で保存する。日本標準時は`540`となる。
- `windows_time_service`: Windows Timeサービスの現在状態を保存する。
- `hardware_clock`: Windowsのシステム時計とは独立して取得できたハードウェアRTCの情報を保存する。

`windows_time_service`の値は`stopped`、`start_pending`、`stop_pending`、`running`、`continue_pending`、`pause_pending`、`paused`、`unknown`のいずれかとする。

Windows実装では、システム時刻に`GetSystemTimePreciseAsFileTime`、UTCオフセットに`GetDynamicTimeZoneInformation`、Windows Timeサービスにサービス制御マネージャーの`QueryServiceStatusEx`を使用する。

通常のWindowsユーザーモードAPIでは、Windowsが管理するシステム時計とは独立してハードウェアRTCを直接読み取れない。このため初期実装では`hardware_clock`を`null`とし、`/clock/hardware_clock`へ`unsupported`と`hardware_clock_direct_access_unsupported`を記録する。システム時刻をRTC値として流用しない。

ハードウェアRTC以外の時計情報をすべて取得できた場合も、RTCが未取得であることを隠さず時計コレクターを`partial`とする。将来、安全で再現性のあるRTC取得方式を採用した場合は、`hardware_clock.time_utc`へUTC日時を保存する。

## CPUカテゴリ

CPU情報は`cpu`へ保存し、コレクター名は`cpu`とする。複数ソケットを扱えるよう、PC全体のトポロジーと物理CPUパッケージごとの情報を分ける。

```json
{
  "cpu": {
    "architecture": "x86_64",
    "topology": {
      "physical_packages": 1,
      "physical_cores": 14,
      "logical_processors": 20
    },
    "packages": [
      {
        "package_index": 0,
        "manufacturer": "GenuineIntel",
        "model": "13th Gen Intel(R) Core(TM) i5-13500",
        "physical_cores": 14,
        "logical_processors": 20
      }
    ],
    "features": {
      "available_instruction_sets": [
        "sse2",
        "sse3",
        "ssse3",
        "sse4_1",
        "sse4_2",
        "avx",
        "avx2",
        "aes"
      ],
      "hardware_virtualization_extensions_available": true,
      "virtualization_firmware_enabled": true,
      "hypervisor_present": false
    }
  }
}
```

### アーキテクチャ

`architecture`は`x86`、`x86_64`、`arm`、`arm64`、`unknown`のいずれかとし、Windows基本情報と同じ列挙値を使用する。

### トポロジー

- `physical_packages`: Windowsが認識した物理CPUパッケージ数。
- `physical_cores`: Windowsが認識した物理コア総数。
- `logical_processors`: Windowsが認識した論理プロセッサー総数。

いずれも取得できた場合は1以上とする。物理コア数は物理CPU数以上、論理プロセッサー数は物理コア数以上でなければならない。

`packages`の各要素:

- `package_index`: 収集結果内で割り当てる0始まりの識別番号。永続的な機器識別子としては使用しない。
- `manufacturer`: CPUが報告したメーカー識別文字列。
- `model`: CPUが報告したモデル名またはブランド文字列。
- `physical_cores`: 対象パッケージに属する物理コア数。
- `logical_processors`: 対象パッケージに属する論理プロセッサー数。

`packages`が1件以上の場合、`package_index`は重複してはならない。すべてのパッケージでコア数を取得できた場合、その合計は`topology`の対応する総数と一致しなければならない。

`topology`オブジェクトとその3つのキーは省略しない。個別の集計値を取得できない場合は、その値を`null`として理由を記録する。`physical_packages`と`packages`を両方取得できた場合は、物理CPU数と配列件数が一致しなければならない。

`packages`の意味:

- 1件以上: 列挙に成功し、Windowsが認識した物理CPUパッケージを保存した。
- 空配列: 列挙には成功したが、物理CPUパッケージを検出しなかった。
- `null`: CPUトポロジーを列挙できなかった、対応していない、または収集を実行しなかった。

### 基本機能

- `available_instruction_sets`: 収集時のWindows環境で利用可能と判定した命令セット名を保存する。配列は重複を許さず、名前は小文字のsnake_caseとする。
- `hardware_virtualization_extensions_available`: 現在の実行環境からIntel VMXまたはAMD SVMなどのハードウェア仮想化拡張が利用可能とCPUIDが報告しているかを保存する。CPU製品自体の対応可否を表す値ではない。
- `virtualization_firmware_enabled`: ハードウェア仮想化機能がBIOS・UEFIで有効とWindowsが報告しているかを保存する。
- `hypervisor_present`: 現在の実行環境でハイパーバイザーの存在をCPUIDが報告しているかを保存する。

`features`オブジェクトと4つのキーは省略しない。機能一覧全体または個別の仮想化状態を判定できない場合は、対応する値を`null`として理由を記録する。`available_instruction_sets`が空配列の場合は、判定に成功したが初期実装で定義した命令セットを検出しなかったことを表す。

`available_instruction_sets`はCPUの製品仕様一覧ではなく、収集時のWindows環境で実際に利用可能な基本機能を表す。初期実装で使用できる値は、x86・x64の`sse2`、`sse3`、`ssse3`、`sse4_1`、`sse4_2`、`avx`、`avx2`、`aes`、`sha`と、ARM・ARM64の`neon`、`arm_v8_crypto`とする。

### Windows実装方針

- トポロジーは`GetLogicalProcessorInformationEx`を使用してパッケージ、物理コア、論理プロセッサーを対応付ける。
- アーキテクチャは`GetNativeSystemInfo`を使用する。
- x86・x64のメーカー、モデル、命令セット、ハードウェア仮想化拡張の公開状態、ハイパーバイザーの存在はCPUIDを使用する。
- BIOS・UEFIでの仮想化有効状態は`IsProcessorFeaturePresent`を使用する。
- 複数パッケージでは各パッケージを代表する論理プロセッサー上で識別情報を照会する。確認せずに1つのCPU情報を全パッケージへ複写しない。
- WMI、PowerShell、外部コマンドは初期実装の必須取得経路にしない。

取得できない値は`null`とし、対応する`/cpu/...`のJSON Pointerと理由を`status.json`へ記録する。一部だけ取得できない場合も、取得済み情報を保持してCPUコレクターを`partial`とする。

### 接続デバイス情報との役割分担

CPUデバイスの開始状態や問題コードは、既存の`devices`コレクターがProcessorクラスを含めて収集している。CPUカテゴリでは同じPnP情報を重複保存せず、診断およびレポート作成時に接続デバイス情報を参照する。

### 初期実装の対象外

- CPU固有ID、Processor ID、シリアル番号
- 温度、消費電力、電圧、ファン速度
- 瞬間的なCPU使用率、プロセス別使用率
- 動作クロック、最大クロック、オーバークロック判定
- キャッシュ階層の詳細

## BIOS・UEFIカテゴリ

BIOS・UEFI情報は`firmware`へ保存し、コレクター名は`firmware`とする。機器のシリアル番号、資産番号、UUIDなどの識別情報は保存しない。

```json
{
  "firmware": {
    "vendor": "American Megatrends International, LLC.",
    "version": "1.90",
    "release_date": "2026-07-17",
    "interface_type": "uefi",
    "secure_boot_enabled": true,
    "status": null
  }
}
```

### 各項目

- `vendor`: Windowsが報告するBIOS・UEFIベンダー名。
- `version`: Windowsが報告するBIOS・UEFIバージョン。複数文字列として保存されている場合は、元の順序を保って` / `で連結する。
- `release_date`: BIOS・UEFI公開日。取得値を検証して`YYYY-MM-DD`へ正規化する。
- `interface_type`: 現在のWindowsが起動したファームウェア方式。`bios`、`uefi`、`unknown`のいずれか。
- `secure_boot_enabled`: Windowsが報告するSecure Bootの有効状態。Legacy BIOSでは`null`とし、`not_applicable`を記録する。
- `status`: ファームウェア全体の稼働状態。将来値を保存する場合は`ok`、`degraded`、`error`、`unknown`のいずれかとする。初期実装では信頼できる一般的な取得元を採用しないため`null`とし、`unsupported`を記録する。

`firmware`オブジェクトと6つのキーは省略しない。取得不能値は`null`とし、対応するJSON Pointer、取得状態、理由コードを`status.json`へ記録する。

### Windows実装方針

- `vendor`、`version`、`release_date`は、`HKEY_LOCAL_MACHINE\\HARDWARE\\DESCRIPTION\\System\\BIOS`にWindowsが公開する特定の値だけを読み取る。
- `interface_type`は`GetFirmwareType`を使用する。
- `secure_boot_enabled`はUEFI起動時に、`HKEY_LOCAL_MACHINE\\SYSTEM\\CurrentControlSet\\Control\\SecureBoot\\State`の`UEFISecureBootEnabled`を読み取る。
- WMI、PowerShell、外部コマンドは初期実装の必須取得経路にしない。
- ファームウェア表、任意のレジストリ、生のファームウェア変数は収集しない。

BIOS公開日の元データが空または日付として不正な場合は、推測や補正をせず`null`と`invalid_value`を記録する。Secure Bootの値が存在しない場合も、無効と推測せず`null`と取得状態を記録する。

### 初期実装の対象外

- BIOS・UEFI設定値の列挙および変更
- ファームウェア更新の自動実行
- シリアル番号、資産番号、システムUUID
- SMBIOSテーブルの生データ
- TPMの詳細情報
- BIOSパスワードの状態
- マイクロコードリビジョン
- ベンダー固有ドライバーまたは管理ツールによる情報

これらは機密情報除外、取得方法の一貫性、初期診断での必要性を考慮し、初期スキーマには含めない。

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
        "started": true,
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

ソフトウェアアダプターにはPnPデバイスインスタンス、ハードウェアドライバー、デバイス問題コードが存在しない場合がある。この場合、対応フィールドを`null`とし、取得状態を`not_applicable`として記録する。`not_applicable`だけを含むことは部分的な収集失敗ではなく、GPUコレクターを`success`にできる。

PCI識別子はJSON上では数値で保存する。画面やHTMLレポートでは、必要に応じて`10DE`のような4桁の16進表記へ変換する。

`driver.date`は時刻を含まない日付として`YYYY-MM-DD`形式で保存する。情報源が有効な日付を返さない場合は`null`とする。

Windows実装では、DXGIのAdapter LUIDとSetupAPIの`DEVPKEY_Device_AdapterLuid`を最優先で照合して同一アダプターを特定する。

Adapter LUIDを取得できない環境では、デバイスインスタンスIDから`VEN`、`DEV`、`SUBSYS`、`REV`を解析し、DXGIのPCI識別子と完全一致する候補が1件だけの場合に限って結合する。同じ識別子を持つ候補が複数存在する場合は、誤った情報を結合せず、対象フィールドを`null`とする。

PCI識別子によるフォールバックで一意に結合できた場合、Adapter LUIDを取得できなかったこと自体は収集警告としない。LUIDとPCI識別子の両方が利用できない場合、候補が存在しない場合、または候補が複数存在する場合に限り、照合不能の理由を`status.json`へ記録する。

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

Windows実機で取得値を確認できたため、現在接続されている物理GPUに次の組み込み診断規則を適用する。ソフトウェアおよびリモートアダプターは対象外とする。

| ルールID | 判定条件 | 検出時の重大度 | 推奨コード |
|---|---|---|---|
| `gpu.device_problem` | `problem_code != 0` | `error` | `review_gpu_device_problem` |
| `gpu.adapter_started` | `started == false` | `warning` | `review_gpu_start_state` |
| `gpu.driver_version_available` | ドライバーバージョンが`null`または空文字列 | `warning` | `review_gpu_driver_installation` |
| `gpu.device_instance_id_unique` | 大文字・小文字を区別せず同一のIDが複数存在する | `warning` | `review_gpu_enumeration` |

`gpus`自体を取得できなかった場合は`not_evaluated`、現在接続されている物理GPUがない場合は`not_applicable`とする。問題コードまたは開始状態だけを取得できなかった場合も、異常を示す別のGPUがない限り、その規則を`not_evaluated`とする。

`passed`を含む評価済みのGPU規則では、対象となった各物理GPUの判定値を`evidence`へ記録する。これにより、問題がなかったという判定についても、使用した問題コード、開始状態、ドライバーバージョンを収集値まで追跡できるようにする。`null`は収集根拠として複写せず、取得できなかったパスを`reason.paths`へ記録する。

デバイスインスタンスIDの一意性判定では、Windows上で大文字・小文字の差が同一デバイスを別物として扱う理由にならないため、比較時にASCII大文字へ正規化する。出力する診断根拠には収集時の文字列をそのまま保存する。IDを取得できない物理GPUがあり、既知のIDに重複がない場合は`not_evaluated`とする。

## 接続デバイスカテゴリ

接続デバイスは、現在存在するデバイスだけでなく、Windowsに過去の接続実績が残っているデバイスも収集対象とする。`present`によって現在接続中かどうかを区別する。

### collection.json

```json
{
  "devices": [
    {
      "name": "Example Device",
      "manufacturer": "Example Vendor",
      "class": "USB",
      "class_guid": "{00000000-0000-0000-0000-000000000000}",
      "device_instance_id": "USB\\VID_1234&PID_5678\\...",
      "device_state": {
        "present": true,
        "started": true,
        "problem_code": 0
      },
      "driver": {
        "version": "1.2.3.4",
        "date": "2026-07-17"
      }
    }
  ]
}
```

`devices`の意味:

- 1件以上: デバイス列挙に成功し、Windowsに登録されたデバイスを保存した。
- 空配列: 列挙に成功したが対象デバイスは存在しなかった。
- `null`: 列挙に失敗した、対応していない、または収集を実行しなかった。

初期実装では、デバイスのシリアル番号、コンテナーID、ユーザー名、ボリュームラベルなど、個人または機器を過度に識別する追加情報は収集しない。デバイスインスタンスIDは状態とドライバーを対応付けるための技術情報として収集するが、収集バンドルを外部提供する前の確認対象とする。

### status.json

コレクター名は`devices`とする。列挙に成功して一部のプロパティだけ取得できない場合は`partial`、列挙自体に失敗した場合は`failed`とする。各`null`には`/devices/0/...`形式のJSON Pointerと取得不能理由を記録する。

`device_state.started`はWindowsの`DN_STARTED`フラグを表し、デバイスが開始されているかを示す。これはデバイスが管理上有効か無効かを直接表す値ではない。現在存在しないデバイスでは、開始状態と問題コードを現在値として評価できない。この場合、`device_state.started`と`device_state.problem_code`を`null`とし、取得状態を`not_applicable`、理由コードを`device_not_present`として記録する。これは収集失敗ではないため、ほかの取得不能値がなければデバイスコレクターを`success`にできる。

デバイスインスタンスIDは、同一のデバイス列挙結果内で重複してはならない。すべての`null`値には対応するフィールド取得状態が必要であり、フィールド取得状態が指すJSON Pointerは実在する`null`値と一致しなければならない。

### 初期実装の収集元

WindowsではSetupAPIの全デバイスクラスを列挙し、統一デバイスプロパティから名称、メーカー、クラス、存在状態、問題コード、ドライバー情報を取得する。現在存在しないデバイスも対象とするため、列挙時に`DIGCF_PRESENT`だけへ限定しない。

### diagnosis.json

接続デバイス診断では、`device_state.present == true`のデバイスだけを評価対象とする。Windowsに過去の接続実績だけが残っているデバイスは、個別規則の判定対象に含めない。

| ルールID | 判定条件 | 検出時の重大度 | 推奨コード |
|---|---|---|---|
| `device.device_problem` | `problem_code == 22` | `warning` | `enable_device` |
| `device.device_problem` | 22以外の`problem_code != 0` | `error` | `review_device_problem` |

`devices`自体を取得できなかった場合は`not_evaluated`とする。現在接続中のデバイスがない場合は`not_applicable`とする。

問題コードを取得できない現在接続中のデバイスがあり、取得済みの値に異常がない場合は`not_evaluated`とする。別のデバイスで異常を検出できた場合は`triggered`とし、取得不能パスも`reason.paths`へ記録する。問題コード22だけが存在する場合は`warning`、22以外の問題コードが1件でも存在する場合は`error`とする。

`passed`を含む評価済み規則では、現在接続中の各デバイスについて取得できた問題コードを`evidence`へ記録する。`null`は収集根拠へ複写しない。過去接続デバイスの値は診断根拠へ含めない。

`started == false`はシステムリソースなどでも正常に発生するため、単独では診断異常としない。ドライバーバージョンの`null`もデバイス種別によって正常に発生するため、初期診断規則には使用せず、取得状態として`status.json`とレポートで扱う。将来、デバイスクラスごとの正常状態を定義できた場合は、開始状態またはドライバー情報を使用する診断規則を再検討できる。

## 物理ディスクカテゴリ

物理ディスクの基本情報は`storage.disks`へ保存する。初期実装ではWindowsの`PhysicalDrive`を照会し、次の項目を収集する。

- `number`: Windowsが割り当てた物理ディスク番号
- `model`: 製品名またはモデル名
- `manufacturer`: メーカー名
- `firmware_revision`: ファームウェアリビジョン
- `bus_type`: Windowsが報告する接続方式
- `capacity_bytes`: ディスク全体の容量（バイト）
- `logical_sector_size_bytes`: 論理セクターサイズ（バイト）
- `removable`: リムーバブルメディアとして報告されているか

```json
{
  "storage": {
    "disks": [
      {
        "number": 2,
        "model": "Example USB Disk",
        "manufacturer": "Example Vendor",
        "firmware_revision": "1.0",
        "bus_type": "usb",
        "capacity_bytes": 32000000000,
        "logical_sector_size_bytes": 512,
        "removable": true
      }
    ]
  }
}
```

`bus_type`は、`scsi`、`atapi`、`ata`、`ieee1394`、`ssa`、`fibre`、`usb`、`raid`、`iscsi`、`sas`、`sata`、`sd`、`mmc`、`virtual`、`file_backed_virtual`、`storage_spaces`、`nvme`、`storage_class_memory`、`ufs`、`unknown`のいずれかとする。

ディスク番号は同一の収集結果内で重複してはならない。容量と論理セクターサイズは、取得できた場合は0より大きい値とする。取得できない項目は`null`とし、`/storage/disks/0/...`形式のJSON Pointerと理由を`status.json`へ記録する。

コレクター名は`physical_disks`とする。物理ディスクの列挙に成功して一部の照会だけ失敗した場合は`partial`、列挙を実行できない場合は`failed`とする。

初期実装ではシリアル番号を収集しない。ストレージデバイス記述子にシリアル番号が含まれていても読み取らず、JSONへ保存しない。

## パーティションカテゴリ

パーティションは`storage.partitions`へ保存し、`disk_number`で`storage.disks`と関連付ける。

```json
{
  "disk_number": 0,
  "partition_number": 1,
  "offset_bytes": 1048576,
  "length_bytes": 999989182464,
  "style": "gpt",
  "type_id": "{ebd0a0a2-b9e5-4433-87c0-68b6b72699c7}",
  "bootable": null
}
```

- `style`は`mbr`、`gpt`、`raw`のいずれかとする。
- `type_id`は、MBRでは`0x07`形式のパーティションタイプ、GPTでは波括弧付きGUIDとする。RAWでは`null`とする。
- `bootable`はMBRのブートインジケーターを保存する。GPTおよびRAWでは`null`とし、`not_applicable`を記録する。
- ディスク番号とパーティション番号の組み合わせは一意とする。
- オフセットと長さは、対応する物理ディスクの容量内に収まらなければならない。

コレクター名は`partitions`とする。Windowsでは`IOCTL_DISK_GET_DRIVE_LAYOUT_EX`を使用する。

## ボリュームカテゴリ

Windowsが認識しているボリュームは`storage.volumes`へ保存する。

```json
{
  "mount_points": ["C:\\"],
  "file_system": "NTFS",
  "capacity_bytes": 999000000000,
  "free_bytes": 500000000000,
  "extents": [
    {
      "disk_number": 0,
      "offset_bytes": 1048576,
      "length_bytes": 999000000000
    }
  ]
}
```

- `mount_points`は初期実装ではドライブ文字のルートだけを保存する。ユーザー固有のディレクトリへ設定されたマウントポイントは機密情報混入を避けるため保存しない。正常に取得できたがドライブ文字を持たない場合は空配列、照会に失敗した場合は`null`とする。
- ボリュームラベル、ボリュームGUID、ボリュームシリアル番号は保存しない。
- `file_system`にはWindowsが報告するファイルシステム名を保存する。
- `capacity_bytes`と`free_bytes`はバイト単位とし、空き容量は総容量を超えてはならない。
- `extents`はボリュームが使用する物理ディスク上の範囲を表す。複数ディスクにまたがるボリュームにも対応するため配列とする。
- ドライブ文字を持たないシステムパーティションや回復パーティションも除外せず、`mount_points`を空配列として保存する。

コレクター名は`volumes`とする。列挙したボリュームの一部でファイルシステム、容量、空き容量、またはディスク範囲を取得できない場合は、取得済み情報を保持したまま`partial`とする。

## SMART・ヘルス情報カテゴリ

ディスクのSMART・ヘルス情報は`storage.smart`へ保存し、`disk_number`で`storage.disks`と関連付ける。コレクター名は`smart`とする。

```json
{
  "disk_number": 1,
  "protocol": "nvme",
  "predict_failure": null,
  "critical_warning": 0,
  "temperature_celsius": 38,
  "available_spare_percent": 100,
  "percentage_used": 4,
  "power_on_hours": 1200,
  "unsafe_shutdowns": 2,
  "media_errors": 0
}
```

`protocol`の値:

- `nvme`: NVMe SMART / Health Information Logを取得した。
- `failure_prediction`: Windowsストレージ層の故障予測状態を取得した。
- `unknown`: どちらの照会方法でも情報を取得できなかった。

NVMeでは`critical_warning`を機器が報告したビットマスクのまま保存する。温度はケルビンから摂氏へ変換し、標準化された予備領域率、使用率、稼働時間、異常終了回数、メディアエラー数を保存する。128ビットのNVMeカウンターがJSONの64ビット整数範囲を超える場合は`null`とし、`invalid_value`を記録する。

Windowsストレージ層の故障予測では`predict_failure`を保存する。NVMe固有項目は`null`とし、`not_applicable`を記録する。ベンダー固有の512バイトは解釈や外部出力をせず、保存しない。

通常権限でも収集可能な情報は取得する。SMART照会が権限不足で拒否された場合は、該当項目を`null`、取得状態を`permission_denied`、理由コードを`smart_permission_denied`として記録し、ほかの収集処理は継続する。管理者権限の有無を事前判定せず、各Windows APIの実際の結果を記録する。

機器またはUSB変換ブリッジがSMART転送に対応していない場合は`unsupported`、権限不足と非対応以外のエラーは`failed`として記録する。

USB変換ブリッジのベンダー固有プロトコルによるSMART取得は初期実装の対象外とし、今後の拡張課題とする。Windowsの標準的な照会で取得できない場合は、ほかの診断ソフトウェアで取得可能な機器であっても`unsupported`として記録する。

### diagnosis.json

初期ストレージ診断では次の4規則を使用する。

| ルールID | 判定条件 | 検出時の重大度 | 推奨コード |
|---|---|---|---|
| `storage.smart_failure_prediction` | `predict_failure == true` | `critical` | `backup_and_replace_disk` |
| `storage.nvme_critical_warning` | `critical_warning != 0` | `error` | `review_nvme_health` |
| `storage.nvme_percentage_used` | `percentage_used >= 100` | `warning` | `plan_nvme_replacement` |
| `storage.volume_free_space` | 空き容量が10%未満、かつ10 GiB未満 | `warning` | `free_volume_space` |

SMART規則は対応するプロトコルの情報を取得できたディスクだけを評価する。対応ディスクが存在しない場合は`not_applicable`、SMART情報または必要な値を取得できない場合は`not_evaluated`とする。別のディスクで異常を検出できた場合は`triggered`とし、取得不能パスも`reason.paths`へ記録する。SMART取得不能そのものは故障として数えない。

`media_errors`、`power_on_hours`、`unsafe_shutdowns`、`temperature_celsius`、`available_spare_percent`は収集・レポート表示の対象とするが、単一時点の汎用的な故障判定には使用しない。将来、機種固有の閾値または複数回の収集結果を比較できる場合に再検討する。

空き容量規則は、ドライブ文字を1件以上持ち、`capacity_bytes`と`free_bytes`を取得できたボリュームを対象とする。ドライブ文字のないEFI、回復、予約パーティションは対象外とする。判定では次のAND条件を使用する。

```text
free_bytes / capacity_bytes * 100 < 10.0
かつ
free_bytes < 10,737,418,240 bytes
```

対象ボリュームの必要値を取得できず、ほかのボリュームで空き容量不足を検出していない場合は`not_evaluated`とする。正常・異常のいずれでも、評価できた容量、空き容量、算出した空き容量率を`evidence`へ記録する。

パーティション番号の重複、ディスク範囲外、同一ディスク上のパーティション範囲重複、ボリューム範囲外、空き容量が総容量を超える状態は、故障診断ではなく収集成果物の整合性検証エラーとして扱う。MBRの拡張パーティション型`0x05`、`0x0F`、`0x85`は論理パーティションを包含するコンテナーであるため、そのコンテナー範囲は重複検証から除外する。パーティションがないディスク、RAWディスク、未割り当て領域、ドライブ文字のないボリューム、複数ディスクにまたがるボリュームは異常としない。

## 未決定事項

- 診断結果の正式な重大度体系
- 診断ルールIDの命名規則
- 診断ルールセットのバージョン規則
- 診断根拠の`value`が取り得るJSON型
- 診断条件の演算子一覧
- 配列要素へのJSON Pointerを長期的に安定させる方法
- メモリモジュール単位の情報を初期実装へ含めるか
