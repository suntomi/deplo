## Deplo
Deploは、あらゆるCI/CDサービスにおいて、全く新しい、統一されたCI/CD開発体験を提供することを目的としたコマンドラインツールです。

Deploは、CI/CDワークフローを設定するためのコマンドラインツールとしてローカル環境で動作し、CI/CDサービスでは、ジョブの成果物をリポジトリに自動プッシュしたり、動的に生成されたキーと値のペアを他のジョブに渡したりといった、実際のジョブに豊かな機能を追加するためのジョブ実行ツールとして動作します。


### Motivation
近年、ソフトウェアやサービスを開発し、本番稼動させるためには、CI/CDが欠かせません。CI/CDの改善により、最近のソフトウェアやサービスの開発プロセスは大きく改善されましたが、それに比べてCI/CDの開発そのものはほとんど改善されていません。

Deploは、以下の3つの問題を解決することで、現在のCI/CDの開発手法自体に一石を投じます。
1. イテレーション
2. コンフィギュレーション
3. 再利用性

#### イテレーション
CI/CD開発における最大の問題は、その時間のかかり方です。2022年になっても、新しいデプロイメントワークフローをテストするためにダミーの修正をプッシュしなければなりません。(多くの場合）自分のジョブが開始する前に多くの依存するジョブが実行される必要があるため、何度も何度も長い時間待つ必要があるのです。

Deplo には、ターゲットコミットによって変更されたファイルによって実行されるジョブをフィルタリングする機能が組み込まれています。しかし、これだけでは反復の時間を短縮することはできません。

この問題を解決するために、CI/CDのジョブをローカル環境で個別に実行できるようにしました。 これは、あたかもターゲットとなるCI/CDサービス上で実行されているかのように、deploによって設定されます。Bitrise CLI](https://app.bitrise.io/cli)が行っているアプローチに似ていますが、deploは単一のCI/CDサービスに縛られることはないのです。

もちろん、deploのローカル環境はまだ100%完全ではありません。ローカル環境と実際のCI/CDサービスのわずかな違いをテストするために、deploでは単一のCI/CDジョブをターゲットCI/CDサービス上で個別に実行できます（これをリモートジョブ実行と呼びます）。

また、DeploではCI/CDサービスに対してsshセッションを提供し、「障害発生直後の調査」にも対応しています。circle CIの組み込み機能とは異なり、CI/CDジョブが失敗した直後にsshセッションを作成するように、たった一行の設定だけでDeploに設定することができます。

これらの機能により、CI/CD開発におけるトライ＆エラーの繰り返し時間を大幅に短縮することができます。

ボーナスとして、ローカル環境からプロジェクト内のCI/CDジョブを実行できるのは、サービスのトラブルシューティングが必要なときに非常に役に立ちます。なぜなら、このような状況で、問題を解決するために通常のリリースフロー（PRの作成、レビュー、マージ、タグの作成など）を使用すると、時間の無駄になるか、別のミスを引き起こす可能性が高いからです。

#### コンフィギュレーション
多くのCIサービスでは、設定フォーマットにyamlを使用しています。このフォーマットは、プロジェクトが小規模でシンプルである場合には便利ですが、大規模なものでは悪夢となります。

Deploは[toml](https://github.com/alexcrichton/toml-rs)という、簡潔で現代的な、より良い構造のテキスト形式を採用しており、独自の複数行インラインテーブルをサポートしています。このため、エラーが起こりやすいyaml形式で設定を書く必要はなく、Deplo.tomlから自動的に生成されます。

[Deplo.toml](https://github.com/suntomi/deplo/blob/main/Deplo.toml) と [.github/workflows/deplo-main.yaml](https://github.com/suntomi/deplo/blob/main/.github/workflows/deplo-main.yml) を比較すると、どのように改善されたかが分かります。

#### 再利用性
多くのCIサービスでは、Github ActionのアクションやCircle CIのオーブのような独自のモジュールシステムがあり、ルーチンワーク（例えば、リポジトリからコードをチェックアウトする、コンテナリポジトリにイメージを送信する）を繰り返し書くことを回避することができます。

しかし、これらのモジュールを使い始めると、ジョブがローカルなテスト性を失ってしまうという大きな欠点があります。なぜなら、これらのモジュールは自社サービス以外の環境で動作するように設計されていないからです。すでに説明したように、ローカルテストの可能性を失うと、CI/CD開発の反復期間が大幅に遅くなってしまいます。

Deploは、サークルCIやgithubアクションと同じ機能を持ちながら、各CIサービスにロックされず、ローカルに実行できる新しいモジュールシステムを提供し、CI/CD開発に必要なローカルなテスト容易性を失わずに再利用性を確保します。



### Glossary
- `リリース環境` 開発中のソフトウェアの異なるリビジョンを提供するために用意されたリソース（サーバーインフラ、バイナリダウンロードのパスなど）のグループに与えられる名前です。

- `リリースターゲット` ある`リリース環境`と同期しているブランチまたはタグ。例えば、 `main` ブランチが更新されると `dev` という `リリース環境` が更新されるように設定されている場合、 `main` は `リリース環境` `dev` の `リリースターゲット` と呼ばれ、 `main` へのすべての変更は `リリース環境` `dev` と同期される。

- `開発ブランチ` 各開発者が日々の開発の成果として実際のコミットを追加するブランチで、プルリクエストを作成して `リリースターゲット` のいずれかにマージする必要があります。デフォルトでは、deploは`開発ブランチ`から`リリースターゲット`へのプルリクエストを作成したときにのみジョブを実行します。

- `変更セット` `リリースターゲット` または `開発ブランチ` が行った、リポジトリに対する実際の変更。deploは変更セットのパスを検出し、この変更に対して実行する必要がある `ジョブ` をフィルタリングします。

- `ジョブ` `リリースターゲット` や `開発ブランチ` が作成されたり更新されたときに実行されるシェルスクリプトです。ジョブは `リリースターゲット` や `開発ブランチ` が更新されたときに起動されるかどうかによって `デプロイジョブ` と `統合ジョブ` に分類され、チェンジセットのコンテンツでフィルタされます。例えば、このリポジトリでは `tools/docker/Dockerfile.builder` がいずれかの `リリースターゲット` に対して更新されて初めて、 `deploy.builder` ジョブが起動することになります。

- `デプロイワークフロー` `リリースターゲット`が更新されたときに実行される`ジョブ`のセットです。
- `デプロイジョブ` `デプロイワークフロー`に関連する`ジョブ`。Deplo.toml の `[jobs.$name]` のようなセクションと`on = { workflows = ["deploy"],... }`のようなトリガー条件で定義する。

- `統合ワークフロー` `開発ブランチ` が作成または更新されたときに実行されるジョブのセットです。
- `統合ジョブ` `統合ワークフロー` に関連する`ジョブ`。Deplo.toml の `[jobs.$name]` のようなセクションと`on = { workflows = ["integrate"],... }`のようなトリガー条件で定義する。


### Install Deplo
#### サポートされているOS
- Linux
- MacOS
- Windows
  - now only confirmed on git-bash.exe

#### 必要なツール
- curl
- git
- docker

#### install binary
``` bash
# macos/linux
curl -L https://github.com/suntomi/deplo/releases/download/${version_to_install}/deplo-$(uname -s) -o /usr/local/bin/deplo
chmod +x /usr/local/bin/deplo
# windows (on git-bash)
curl -L https://github.com/suntomi/deplo/releases/download/${version_to_install}/deplo-Windows.exe -o /usr/bin/deplo
chmod +x /usr/bin/deplo
```



### Configure your project CI/CD to use Deplo
#### overview
1. Deplo.tomlの作成
2. .envの作成
3. deplo initの実行

#### Deplo.tomlの作成
- example https://github.com/suntomi/deplo/blob/main/Deplo.toml
- 説明は設定ファイルのコメントをご覧ください

#### .envの作成
deploは、環境変数として機密性の高い値を注入するための.envファイルをサポートしています。ローカルホストで実行する場合は、`Deplo.toml`に直接secretを記述する代わりに、これを使用することを強くお勧めします。deploは自動的にリポジトリのルートから.envを検索し(または`-e`オプションでパスを指定)、`Deplo.toml`の中で環境変数名を参照する形でsecretの注入に使用します。CIサービス上で動作させる場合、`deplo init` または `deplo ci setenv` が起動されると、各CIサービスのsecretとして .env の内容をアップロードします。

#### deplo initの実行
- すでにdeploをインストールしている場合
  ``` bash
  deplo init
  # this will create Deplo.toml, .circleci configuration, .github/workflows configuration, and ./deplow
  ```

- deploをインストールしていなくて、インストールをしたくない場合、docker imageを使って最初の実行をすることができます。
  ``` bash
  docker run --rm -ti -v $(pwd):/workdir -w /workdir ghcr.io/suntomi/deplo:latest init
  ```

- すると、リポジトリのルートに `deplow` というスクリプトが作成され、チームメンバーは `deplow` を使って、deplo がグローバルにインストールされていなくても deplo を呼び出すことができます。バージョンの偏りを避けるために、./deplow があればそれを使用することを推奨します。
  ``` bash
  ./deplow info version
  # should prints its version code
  ```



### deplo jobを実行する
大きく３通りがあります

1. from CI service
2. from command line (Local)
3. from command line (Remote)

#### from CI
おそらく皆さんにとって最も身近なものでしょう。`リリース対象`に対して push または pull リクエストがあると、github actions または circle ci がワークフローを開始します。
ワークフローでは、 `deplo boot` が実行され、Deplo.toml で定義された `ジョブ` が、push または pull request に含まれる `変更セット` に従って実行されます。

ローカル環境でも `deplo boot` を実行することができますが、`リリースターゲット` にいない場合は、`deplo -r nightly boot` のように `-r` オプションを指定することを忘れないようにしてください。

#### from command line (Local)
Deplo.tomlで定義された各ジョブは、 `deplo i -r $release_target $job_name` (`統合ジョブ`) または `deplo d -r $release_target $job_name` (`デプロイジョブ`) を使用して個別に実行することができます。`-r $release_target` はinfraの更新やdeployなどの`デプロイジョブ`では更新する対象を指定する必要があるので必須ですが、ドキュメントの生成など、リリースターゲットを必要としない場合には省略できます。
また、対応するジョブを実行する環境と様々な形でやりとりをすることができます。

シェルにログイン: `deplo i -r $release_target $job_name sh`

任意のコマンドの実行: `deplo i -r $release_target $job_name sh ${adhoc command args}`

あらかじめ定義されたコマンドライン引数の実行: `deplo i -r $release_target $job_name sh @task_anme`

任意の環境変数も追加で設定できます: `deplo i -r $release_target $job_name -e ENV1=VAR1`。

またはコミットSHAを指定してジョブを実行します: `deplo i -r $release_target $job_name --ref efc6d3e2c1a1d875517bf81fb3ac193541050398`

#### from command line (Remote)
__実際のCIサービス環境__でジョブを実行するには、`--remote`を使用します。
コマンド `${adhoc command args}` を `$job_name` の CI サービス環境上でリモートから実行する: `deplo i -r $release_target $job_name --remote sh ${adhoc command args}`


### Roadmap
[see here](https://github.com/suntomi/deplo/issues/12)
