cli/src/command/vcs.rs の control_pr では　deplo vcs pr merge/close というpull requestに対して操作を行う２つのサブコマンドが実装されています。ここにdeplo vcs pr createというpull requestを作成するコマンドを追加してください。

PRの作成に必要なパラメーターは、例えばdeplo vcs pr merge のように -o "key=value" の形式で指定できるようにします。

```
merge pull request

Usage: cli vcs pr merge [OPTIONS] <url>

Arguments:
  <url>  URL of the pull request

Options:
  -o <option>      option for pull request merge.
                   -o $key=$value
                   for github, body options of https://docs.github.com/en/rest/pulls/pulls?apiVersion=2022-11-28#merge-a-pull-request can be specified.
                   plus, -o auto_merge=true to enable auto merge.
                   -o message=$text to post comment to pull request.
                   -o approve=true to approve the pull request.
                   TODO: for gitlab
  -h, --help       Print help
```
