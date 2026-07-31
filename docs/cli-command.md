# CLI基本仕様

## 引数なし実行

コマンドを省略した場合、次の処理を順番に実行する。

```text
pcdiag.exe
pcdiag.exe --output <出力先ディレクトリ>
```

`--output`を省略した場合は、現在の作業ディレクトリを出力ルートとする。指定した場合は、そのディレクトリを出力ルートとする。

```text
collect → diagnose → report
```

1. `collect`が新しいセッションディレクトリを作成する。
2. 作成したセッションディレクトリを入力として`diagnose`を実行する。
3. 同じセッションディレクトリを入力として`report`を実行する。
4. すべて成功した場合、標準出力へ`report`成果物ディレクトリのパスを出力する。

進捗は標準エラー出力へ表示する。いずれかの処理が失敗した場合は後続処理を実行せず、終了コード`1`で終了する。失敗前に完成した成果物は削除しない。

引数の解析に失敗した場合は終了コード`2`、`--help`およびすべての処理が成功した場合は終了コード`0`とする。

## 成果物の取り扱いに関する注意

引数なしの一括実行と、`collect`、`diagnose`および`report`の個別実行では、処理の開始時に成果物の取り扱いに関する注意を標準エラー出力へ一度表示する。一括実行では各工程ごとに繰り返さない。`--help`および引数解析に失敗して成果物を扱わない場合は表示しない。

標準出力は成功時の成果物パスだけを出力する既存の形式を維持する。表示内容と機微情報の取り扱い方針は、[`sensitive-data-policy.md`](sensitive-data-policy.md)を参照する。

## 個別実行

```text
pcdiag.exe collect --output <出力先ディレクトリ>
pcdiag.exe diagnose --output <セッションディレクトリ>
pcdiag.exe report --output <セッションディレクトリ>
```

個別実行の成果物形式と上書き防止規則は、引数なし実行でも変更しない。

## Windows Update履歴の収集オプション

Windows Update履歴は、既定で収集開始時刻から過去180日間、最大1,000件を新しい順に取得する。引数なしの一括実行と`collect`コマンドで、次のオプションを指定できる。

```text
--windows-update-days <日数|all>
--windows-update-max-entries <件数|all>
--windows-update-all
```

- `--windows-update-days`は1から3,650の日数、または期間制限を設けない`all`を受け付ける。
- `--windows-update-max-entries`は1から100,000の件数、または件数制限を設けない`all`を受け付ける。
- `--windows-update-all`は期間と件数の両方を無制限にする。
- 後に指定した個別オプションは`--windows-update-all`の設定を上書きできる。

例:

```text
pcdiag.exe collect --output results --windows-update-days 90 --windows-update-max-entries 500
pcdiag.exe collect --output results --windows-update-all
```

期間または件数により履歴を打ち切った場合は、`status.json`のWindows Updateコレクターへ切り捨て理由を記録する。
