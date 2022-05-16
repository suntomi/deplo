# about secret management

secretは

- env or fileから取得する(local)
- 何らかのsecret managerから取得する(remote)

local typeはdeplo setenvでCI側にアップロードしておきCIではすべて環境変数として参照する
remote typeはlocalでもCIでも同じように、何らかの方法で認証を行う

deplo setenv => local typeのsecretをCIに移動させる