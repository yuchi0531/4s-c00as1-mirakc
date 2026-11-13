# mirakc WebUI (静的 SPA)

`~/projects/mirakc-webui` のビルド成果物 `dist/` を、mirakc の
`server.mounts` 経由で `/www` に公開する。

## ビルド

```sh
cd ~/projects/mirakc-webui
npm run build
```

成果物:

```text
dist/
  index.html
  favicon.svg
  assets/index-*.js
```

`vite.config.ts` で `base: './'` を指定しているため、マウント先のパスに
依存せず相対パスでアセットを解決できる。ビルド後は `dist/index.html` が
`./assets/...` を参照していることを確認する。

## 実機への配置

実機の chroot 内では `/opt/t4s/www`、ホスト側からは
`/mnt/t4s/opt/t4s/www` に配置する。

```sh
# ホスト側で tar を作る
tar -C ~/projects/mirakc-webui/dist -cf .tmp/http/www.tar .

# 実機側（telnet 経由）
mkdir -p /mnt/t4s/opt/t4s/www
wget -T 20 -q http://192.168.100.100:8080/www.tar -O /tmp/www.tar
tar -xf /tmp/www.tar -C /mnt/t4s/opt/t4s/www
chown -R 0:0 /mnt/t4s/opt/t4s/www
chmod 755 /mnt/t4s/opt/t4s/www /mnt/t4s/opt/t4s/www/assets
find /mnt/t4s/opt/t4s/www -type f -exec chmod 644 {} \;
```

`scripts/deploy.sh` は `~/projects/mirakc-webui/dist` が存在する場合、
上記の `www.tar` 転送と展開も行う。

## mirakc 設定

`config/config.yml`（および `config/config.full.yml`）の `server:` に
以下を追加する。パスは chroot 内の `/opt/t4s/www`。

```yaml
server:
  addrs:
    - http: '0.0.0.0:40772'
  mounts:
    /www:
      path: /opt/t4s/www
      index: index.html
```

実機では `/mnt/t4s/opt/t4s/config.yml` に反映する。`server.mounts` の
変更は mirakc の再起動が必要。再起動前に
`/api/tuners` の `isUsing` が `false` であることを確認する。

## URL と確認

```text
http://192.168.1.154:40772/www          -> 303 Location: /www/index.html
http://192.168.1.154:40772/www/index.html -> 200 text/html
http://192.168.1.154:40772/api/version  -> 200 application/json
```

`/www`（末尾スラッシュなし）を使う。mirakc 4.0.0-dev.0 の
directory mount では `/www/` は 404 になり、`index` はマウントルート
`/www` へのアクセス時に `/www/index.html` へリダイレクトする。
`curl -L` では最終的に 200 になる。

## 制約

- **same-origin**: SPA は mirakc と同じオリジンから配信する。API は
  相対パス `/api`、SSE は `/events` を使うため、別ホスト・別ポートから
  配信すると CORS やイベント接続の問題が起きる。
- **HashRouter**: 画面遷移は `#/...` で行う。サーバ側の SPA fallback は
  不要で、`/www/index.html` 以外のパスを直接開く必要はない。
- **設定編集なし**: WebUI は参照専用で、mirakc の設定変更・保存は行わない。
- **静的ファイルのみ**: `dist/` にビルド済みファイルを置く。`npm run dev`
  は開発用で、実機では使わない。
- 認証はない。信頼できる LAN 内に限定して公開する。