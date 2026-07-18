# reportコマンド仕様

## 目的

`collect`で生成した収集成果物と`diagnose`で生成した診断成果物を、人がブラウザーで確認しやすい単一のHTMLへ変換する。

初期実装ではPDF生成を対象外とする。HTMLはブラウザーの印刷機能から印刷またはPDF保存できる構成とするが、pcdiag自身はPDFファイルを生成しない。

## コマンド

```text
pcdiag.exe report --output <セッションディレクトリ>
```

`--output`には、`collection`および`diagnosis`ディレクトリを持つ既存のセッションディレクトリを指定する。

## 入力

```text
<session>/
├── collection/
│   ├── collection.json
│   ├── status.json
│   └── manifest.json
└── diagnosis/
    ├── diagnosis.json
    └── manifest.json
```

生成前に次を検証する。

- 両成果物のマニフェストとJSON形式
- マニフェストに記録されたファイルサイズとSHA-256
- 宣言されていないファイルが成果物ディレクトリに存在しないこと
- 収集結果と収集状態の整合性
- 診断結果と収集結果の整合性
- 両成果物の`session_id`が一致すること
- 診断成果物の入力IDが収集成果物の`artifact_id`と一致すること

検証に失敗した場合はレポートを生成しない。

## 出力

```text
<session>/report/
├── report.html
└── manifest.json
```

生成中は`report.incomplete`を使用し、全ファイルとマニフェストの生成に成功した後で`report`へ変更する。既存の`report`および`report.incomplete`は上書きしない。

レポートマニフェストは、入力としてcollection成果物とdiagnosis成果物をそれぞれ1件記録する。`report.html`のメディア種別は`text/html; charset=utf-8`とする。

## HTMLの構成

初期実装では次を表示する。

1. セッションIDと診断規則セット
2. 総合判定と重大度別の検出件数
3. 検出事項、根拠、推奨事項コード
4. Windows、CPU、メモリ、ファームウェアの概要
5. GPU一覧
6. 物理ディスクとSMARTの概要
7. 現在接続、過去の接続記録、問題コードありのデバイス件数
8. 情報収集項目ごとの成否、所要時間、補足コード
9. 入力成果物のIDと状態

## 表示と安全性

- UTF-8の単一HTMLとし、CSSをファイル内へ埋め込む。
- JavaScript、外部CSS、外部フォント、画像、ネットワーク通信を使用しない。
- 収集値と診断値はHTMLとして解釈されないようエスケープする。
- JSON全体をそのまま埋め込まない。
- 収集対象外の機密情報を新たに取得または推定しない。
- 未取得値は`取得不能`として表示し、正常値として補完しない。

## 成果物状態

HTMLとレポートマニフェストの生成に成功した場合、レポート成果物の`status`は`complete`とする。入力成果物が`partial`の場合も、レポート生成処理自体が成功していれば`complete`とし、入力成果物の状態と情報収集状況をHTML内に明示する。

## 初期実装の対象外

- PDFファイルの直接生成
- テンプレートやテーマの選択
- JavaScriptを用いた絞り込みや並べ替え
- 収集JSONおよび診断JSONのHTMLへの完全複写
- レポート成果物の上書き更新
