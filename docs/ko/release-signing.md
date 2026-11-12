# 릴리스와 서명

[English](../release-signing.md) | 한국어

메인테이너용입니다: 릴리스가 만들어지는 방식, 그리고 릴리스에 서명하는 키를 보관하고, 바꾸고, 새로 만드는 방법.

## 릴리스가 만들어지는 방식

`.github/workflows/release.yml` 이 빌드, 서명, 게시를 합니다:

- **`main` 에 머지** → beta 사전 릴리스 `vX.Y.Z-beta.N`. `X.Y.Z` 는 다음 stable 버전입니다: 가장 새 stable 태그의 다음 패치, 또는 PR 이 `crates/core/Cargo.toml` 의 버전을 올렸으면 그 버전. `N` 은 머지마다 하나씩 늘어납니다.
- **`main` 에 있는 커밋의 버전 태그** → stable 릴리스 `vX.Y.Z`. latest 로 표시하고, 이전 stable 릴리스 이후의 변경을 노트로 달고, Homebrew formula 도 올립니다. 내보낼 beta 의 커밋에 태그를 붙이세요: `git tag vX.Y.Z <sha> && git push origin vX.Y.Z`. `main` 에 없는 커밋의 태그나 가장 새 stable 릴리스보다 낮은 태그는 거절되고, 실행 로그가 고치는 법을 알려 줍니다 (`git push --delete origin vX.Y.Z` 로 태그를 지우고 맞는 태그를 올리기).
- **워크플로를 바꾸는 Pull request** → 빌드만 하고 아무것도 게시하지 않습니다.

모든 stable 릴리스 노트 맨 위에 macOS 13 경고가 붙습니다. 0.3.0 이전 앱은 "What's Changed" 항목만 보여 주고 어떤 stable 릴리스든 받을 수 있기 때문입니다. 0.3.0 이후 앱은 이 항목을 보여 주지 않습니다. beta 의 `.deb` 는 패키지 버전이 `X.Y.Z~beta.N` 이라서 `dpkg` 와 `apt` 가 stable `X.Y.Z` 보다 낮게 봅니다.

두 채널 모두 같은 파일, 서명, `SHA256SUMS`, 빌드 출처 증명을 붙입니다 ([안전장치](safety.md)).

## 릴리스 서명 키

Qurry 가 네트워크로 설치하는 모든 것을 키 하나가 서명합니다: GUI 의 앱 안 업데이트 번들 (서명은 릴리스마다 `latest.json` 에 들어갑니다) 과 릴리스마다의 `SHA256SUMS` (`SHA256SUMS.sig` 로). `qurry-tui --update` 와 드라이버 플러그인 다운로드가 이 서명을 확인합니다. [Tauri updater](https://v2.tauri.app/plugin/updater/) (minisign) 키 쌍이고, Qurry 의 메인테이너 한 명이 갖고 있습니다.

### 보관 위치

- 개인 키와 그 비밀번호는 이 저장소의 GitHub Actions secret `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 입니다. 커밋하지 않습니다. `.github/workflows/release.yml` 은 이 둘을 `publish` job 의 Sign 단계에만 넘깁니다. 이 job 은 `main` 으로의 push 와 `main` 의 버전 태그에서만 돌고, 빌드 job 이나 풀 리퀘스트에는 넘기지 않습니다.
- secret 은 GitHub 에서 다시 읽을 수 없으니 개인 키 파일과 비밀번호를 GitHub 밖에 오프라인으로 백업해 두세요 (없으면 잃어버린 키를 되찾을 수 없습니다. 아래 참고).
- 공개 키는 저장소 안 두 곳에 있고, 둘이 같아야 합니다 (`crates/tui/src/selfupdate.rs` 의 테스트 `sums_must_be_signed_with_the_update_key` 가 확인합니다):
  - `src-tauri/tauri.conf.json` 의 `plugins.updater.pubkey` (GUI updater, 키 하나)
  - `crates/core/src/update.rs` 의 `UPDATE_PUBKEYS` 첫 번째 키 (`qurry-tui --update` 와 드라이버 플러그인은 이 목록의 아무 키로나 서명된 `SHA256SUMS` 를 받아들입니다). 목록에는 이 키 하나만 있고, 키를 바꾸는 동안에만 옛 키가 그 뒤에 붙습니다.

설치된 버전은 자기가 빌드될 때의 공개 키만 믿으므로, 키를 바꾸려면 다리 (bridge) 릴리스가 필요합니다. 설치된 버전은 앱 안에서 **Latest** 로 표시된 릴리스 (beta 채널이면 가장 새 beta) 로만 업데이트하므로, 다리 (또는 같은 방식으로 빌드된 beta) 가 제안되는 가장 새 릴리스인 동안에만 앱 안에서 다리에 닿습니다.

### 키 바꾸기

1. 새 키 쌍을 만듭니다: `pnpm tauri signer generate -w <file>`, 비밀번호를 붙여서. 오프라인으로 백업합니다.
2. 풀 리퀘스트에서 새 공개 키를 `plugins.updater.pubkey` 에, 그리고 `UPDATE_PUBKEYS` 맨 앞에 넣고 옛 키를 그 뒤에 둡니다. `sums_must_be_signed_with_the_update_key` 의 테스트 서명 (`hello\n` 의 서명) 을 새 키로 다시 서명해 (`pnpm tauri signer sign -f <file> <hello file>`, 그다음 `.sig` 내용을 씁니다) 테스트가 통과하게 합니다.
3. 풀 리퀘스트를 머지하고, secret 에 아직 **옛** 키가 있는 동안 stable 버전으로 릴리스합니다 (`main` 의 머지 커밋에 태그: `git tag vX.Y.Z <sha> && git push origin vX.Y.Z`). 이 다리 릴리스는 옛 키로 서명되므로 설치된 모든 버전이 받아들입니다. 이때부터 다리의 GUI updater 는 새 키만 믿고, `qurry-tui --update` 와 드라이버 플러그인은 둘 다 믿으므로 드라이버 플러그인은 전처럼 다리 자신의 릴리스 (옛 키로 서명) 에서 설치됩니다.
4. 설치된 버전이 앱 안에서 찾을 수 있게 다리를 2주 동안 Latest 로 둡니다. 그동안 다른 stable 버전을 내지 말고 `main` 에 머지하는 것도 피하세요: 머지마다 옛 키로 서명된 beta 가 게시되는데, 다리의 GUI updater 는 더 이상 그 키를 믿지 않으므로, 다리를 가진 beta 채널 GUI 사용자는 5단계의 beta 전까지 앱 안에서 업데이트할 수 없습니다 (`qurry-tui --update` 와 드라이버 플러그인은 두 키를 다 믿으므로 괜찮습니다). 2주는 절충입니다: 길면 더 많은 사람이 앱 안에서 업데이트하고, 짧으면 이 상태가 빨리 끝납니다.
5. 새 키로 바꾸고 다음 stable 버전을 릴리스합니다:
   1. 두 secret 을 새 키로 바꿉니다. 이제부터 모든 릴리스 (beta 와 stable) 가 새 키로 서명됩니다.
   2. 풀 리퀘스트에서 `UPDATE_PUBKEYS` 에서 옛 키를 빼고 (이후 버전은 더 이상 받아들이지 않도록), "개인 키를 잃어버렸을 때" 의 키 안내를 `.github/release-notes.md` 맨 위에 넣고 (이 릴리스를 설치할 수 없는, 다리보다 오래된 버전을 위해), 머지합니다. 그 beta 가 새 키로 서명된 첫 beta 입니다.
   3. 그 머지 커밋 (또는 `main` 의 더 나중 커밋) 에 다리보다 높은 버전으로 태그를 붙입니다: `git tag vX.Y.Z <sha> && git push origin vX.Y.Z`. stable 릴리스는 `main` 의 버전 태그로만 만들어지고, 워크플로는 가장 새 stable 릴리스보다 낮은 태그를 거절합니다. 다리 이후 `main` 에 바뀐 것이 없어도 됩니다.
6. 5단계의 릴리스가 Latest 가 되면 옛 개인 키의 모든 사본을, 오프라인 백업까지 없앱니다. 다리와 같은 방식으로 빌드된 beta 는 `UPDATE_PUBKEYS` 에 옛 키를 갖고 있으므로, 사람들이 그 버전에 머무는 동안 그 `qurry-tui --update` 와 드라이버 플러그인은 옛 키로 서명된 `SHA256SUMS` 를 받아들입니다.

다리 릴리스보다 오래된 버전은 새 키로 서명된 릴리스를 확인할 수 없습니다. 5단계의 릴리스가 Latest 가 되면 그 릴리스만 제안받고, 앱 안 업데이트와 `qurry-tui --update` 가 서명 확인에서 실패하므로 더 이상 앱 안에서 갈 길이 없습니다. 그 버전에 남은 사용자는 아래처럼 다시 설치합니다. 업데이트 창의 키 안내가 그렇게 알려 줍니다.

### 개인 키를 잃어버렸을 때

설치된 버전은 다른 키로 서명된 것을 받아들일 수 없으므로, 앱 안에서 새 키로 넘어갈 길은 없습니다. 새 키 쌍을 만들어 두 secret 과 공개 키 두 곳에 옛 키 대신 넣고 (옛 키는 쓸모없으니 `UPDATE_PUBKEYS` 에는 새 키만), 아래 안내를 `.github/release-notes.md` 에 넣고, 릴리스합니다. 이전 모든 버전의 사용자는 [Releases](https://github.com/Hkhyyy/qurry/releases) 페이지에서 한 번 다시 설치해야 합니다 (GUI: DMG, AppImage, deb. TUI 와 MCP: `brew upgrade qurry` 또는 `qurry-cli_…` 압축 파일). Data Source, 비밀번호, 설정은 그대로입니다. 그 뒤로는 업데이트가 전처럼 됩니다.

업데이트 창에서 알려 주세요. 업데이트 창은 릴리스 노트의 `## What's Changed` 아래 항목만 보여 줍니다 (GUI, TUI, 0.3.0 이전 버전 모두): 옛 버전은 업데이트를 찾았을 때, 설치가 서명 확인에서 실패하기 전에 이것을 보여 줍니다. `.github/release-notes.md` 맨 위에 이것을 넣으세요 (릴리스 노트는 영어라 안내도 영어 그대로입니다. `crates/core/src/update.rs` 의 테스트 `release_signing_key_notice_shows_in_update_window` 가 영어 문서 [release-signing.md](../release-signing.md) 의 이 블록을 읽습니다):

```markdown
## What's Changed
* [breaking] The update signing key changed. If updating fails with a signature error, that is expected, not an attack: download Qurry again from https://github.com/Hkhyyy/qurry/releases (your data sources, passwords and settings are kept)
```

노트의 다른 곳에 쓴 글은 릴리스 페이지에서만 보입니다. 같은 글로 이슈나 토론도 하나 고정해 두세요.

새 키로 서명된 첫 릴리스부터 6개월 동안 안내를 `.github/release-notes.md` 에 두어 모든 릴리스에 붙게 하고, 그 뒤 풀 리퀘스트로 지웁니다 (이슈 고정도 풉니다). 옛 버전은 확인할 때 제안받는 릴리스의 노트를 보여 주는데 그것이 나중의 어느 stable 릴리스든 될 수 있으므로, 사람들이 옛 버전을 열 수 있는 동안에는 모든 릴리스에 안내가 있어야 합니다.

### 개인 키가 새었을 때

새어 나간 키를 가진 사람은, 이 저장소의 릴리스로 내보낼 수만 있다면 설치된 모든 버전이 받아들이는 업데이트에 서명할 수 있습니다.

1. 막기: 저장소와 메인테이너의 GitHub 계정에서 내가 만들지 않은 릴리스, 태그, 워크플로 변경, secret, 액세스 토큰을 찾습니다. 그 릴리스는 지우고 그 토큰은 취소합니다.
2. 바로 키를 바꿉니다. "키 바꾸기" 대로 하되 4단계의 2주를 건너뛰고, 2단계에서 새어 나간 키를 `UPDATE_PUBKEYS` 에 넣지 않습니다 (그러면 다리는 5단계의 릴리스 전까지 새어 나간 키로 서명된 자기 릴리스에서 드라이버 플러그인을 설치할 수 없는데, 5단계는 바로 이어집니다): 다리 릴리스는 여전히 새어 나간 키 (설치된 버전이 받아들이는 유일한 키) 로 서명되고, 다리가 게시되자마자 secret 을 새 키로 바꿉니다. 그다음 새어 나간 키의 모든 사본을 없앱니다. 기간이 없으니 앱 안에서 다리에 닿는 사람은 거의 없으므로, 모든 사용자가 다시 설치한다고 보세요.
3. 사용자에게 알리기: [보안 권고](https://github.com/Hkhyyy/qurry/security/advisories) 를 게시해, 어떤 릴리스가 영향을 받았을 수 있는지, 다리보다 오래된 모든 버전은 [Releases](https://github.com/Hkhyyy/qurry/releases) 페이지에서 다시 설치해야 한다는 것 (거기서는 앱 안 업데이트와 `qurry-tui --update` 가 서명 오류로 실패합니다), 그리고 다운로드한 파일을 빌드 출처 증명과 맞춰 보는 법 (`gh attestation verify <file> --repo Hkhyyy/qurry`) 을 알립니다. "개인 키를 잃어버렸을 때" 의 키 안내는 평소처럼 5단계에서 `.github/release-notes.md` 에 들어갑니다.

다리 릴리스보다 오래된 버전은 다시 설치할 때까지 새어 나간 키를 계속 믿으므로, 권고에 이것을 분명히 적으세요. 새어 나간 키가 예전 다리가 `UPDATE_PUBKEYS` 두 번째에 둔 이전 키라면 (키 바꾸기 6단계에서 없애지 않았다면), 그 다리와 같은 방식으로 빌드된 beta 도 `qurry-tui --update` 와 드라이버 플러그인에서 (GUI updater 는 아니고) 그 키를 믿으므로, 권고에서 그 버전에 남은 사용자에게도 최신 릴리스로 업데이트하라고 알려야 합니다.

---

[← 개발](development.md)
