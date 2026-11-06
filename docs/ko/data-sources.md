# Data Source

[English](../data-sources.md) | 한국어

Qurry 가 다루는 데이터베이스, 연결 방법 (SSH, AWS SSM, SSL, 옵션), 덜 흔한 데이터베이스의 드라이버를 추가하는 방법입니다.

## 지원하는 데이터베이스

| 데이터베이스 | 기능 |
|---|---|
| PostgreSQL | 탐색기 트리 (데이터베이스, 스키마, 테이블, 뷰, 시퀀스, 루틴), 쿼리 콘솔 (서버 커서로 나눠 받기, 취소, 수동 트랜잭션), 데이터 편집, 실행 계획, DDL, 전체 텍스트 검색. `infinity` / `-infinity` 날짜 · 타임스탬프는 그대로 `infinity` / `-infinity` 로, 그 배열은 값으로 보여 줍니다 |
| MySQL / MariaDB | 위와 같음. MySQL 은 데이터베이스가 곧 스키마라서 테이블이 데이터베이스 바로 아래에 보입니다 |
| Redis | Medis 같은 브라우저 (타입 · TTL 이 붙은 키 목록, 타입별 값 편집기), redis-cli 같은 터미널, **Standalone / Sentinel / Cluster** |

연결 옵션:
- **SSH 터널** (키 또는 비밀번호). 호스트 키는 `~/.ssh/known_hosts` 로 확인합니다. 키가 바뀌었으면 거부하고, 처음 보는 서버는 SHA256 지문을 보여 주며 **Trust host key** 를 고를 때까지 멈춥니다 (GUI: Data Source 창, TUI: 폼에서 `⌃y` 또는 `⌃k › Trust SSH Host Key`). 터널은 연결된 동안 `127.0.0.1` 의 임의 포트에서 기다립니다. 같은 컴퓨터의 다른 프로그램도 그 포트를 쓸 수 있으므로, 여럿이 쓰는 컴퓨터에서는 (bastion IP 허용만 믿지 말고) DB 비밀번호를 쓰고 **Auto-disconnect** 를 켜서 쓰지 않는 터널이 닫히게 하세요. SSH 세션 하나를 그 Data Source 의 모든 데이터베이스가 같이 씁니다. 세션이 끝나면 (노트북 잠자기, 네트워크 변경) 다음 쿼리가 다시 로그인합니다.
  - **ssh-agent 와 `~/.ssh/config`** 를 `ssh` 처럼 씁니다. 개인 키를 비워 두면 (또는 Password 를 골라 비워 두면) ssh-agent (`SSH_AUTH_SOCK`) 로, 그다음 `~/.ssh/config` 의 `IdentityFile` 키 (없으면 `~/.ssh/id_ed25519`, `id_ecdsa`, `id_rsa`) 로 로그인합니다. 암호가 걸린 키는 에이전트에 넣어 (`ssh-add`) 씁니다. 에이전트의 키 가운데 `IdentityFile` (그 `.pub`) 과 맞는 키를 먼저 내밀어서, 서버의 시도 횟수 제한 (`MaxAuthTries`, 기본 6) 을 다른 키가 다 써 버리지 않습니다. `IdentitiesOnly yes` 면 그 키만 내밉니다. 키 파일이나 비밀번호를 넣으면 전처럼 그것을 씁니다. SSH 호스트에 `~/.ssh/config` 의 `Host` 별칭을 써도 됩니다. 그 `HostName`, `User` (Data Source 의 사용자가 비었을 때), `Port` (Data Source 의 포트가 기본값 22 일 때), `IdentityFile`, `IdentityAgent`, `IdentitiesOnly`, `ProxyJump` 가 적용되고, `Host` 패턴 (`*`, `?`, `!`), 먼저 나온 값 우선, `Include` 는 OpenSSH 와 같게 읽습니다. **ProxyJump** 체인 (`ProxyJump a,b`, 점프 호스트 자신의 `ProxyJump` 도) 은 호스트를 차례로 거칩니다. 점프 호스트는 에이전트나 자기 `IdentityFile` 키로만 로그인하고, Data Source 의 비밀번호나 키는 그 Data Source 의 SSH 호스트에만 보냅니다. 거치는 모든 호스트를 `known_hosts` 로 확인하고, **Trust host key** 는 지문을 보여 준 호스트를 (점프 호스트여도) 신뢰합니다. Qurry 는 `~/.ssh/config` 의 프로그램을 실행하지 않습니다. `ProxyCommand` 는 그 파일 · 줄 · `Host` 블록과 할 일을 알려 주며 거부합니다 (`ProxyJump` 를 쓰거나, 그 위의 더 좁은 `Host` 블록에 `ProxyCommand none` 을 넣으세요). `Host *` 아래의 `ProxyCommand` (Teleport, cloudflared, 회사 프록시 도구) 는 호스트가 맞는 모든 SSH 터널을 멈춥니다 (자기 키나 비밀번호가 있는 것도). Qurry 가 바로 가는 호스트에는 그 위에 `Host db-bastion` / `ProxyCommand none` 을 넣으세요. 다만 흔한 `ssh -W %h:%p jump-host` 는 `ProxyJump jump-host` 와 같은 길이라 받습니다. `Match` 블록은 건너뜁니다. 값은 먼저 확인합니다: 공백이나 맨 앞의 `-` 가 있는 호스트 이름 · 사용자, 숫자가 아닌 포트가 있으면 파일과 줄을 알려 주며 접속을 멈춥니다. `%h %p %r %u %d %%` 이외의 `%` 토큰이 있는 `IdentityFile` · `IdentityAgent` 는 건너뛰므로 (로그인 오류가 어느 줄인지 알려 줍니다) 자기 키나 비밀번호가 있는 Data Source 를 멈추지 않습니다. 저장된 Data Source 에도 (GUI, TUI, MCP) 적용되므로, SSH 호스트에 `~/.ssh/config` 의 `Host` 블록이 있는 Data Source 는 이제 `ssh` 처럼 그 설정을 씁니다.
- **환경의 PostgreSQL 비밀번호**: 저장된 비밀번호가 없는 Data Source 는 `PGPASSWORD`, 그다음 `~/.pgpass` (또는 `PGPASSFILE`) 에서 자기 호스트 · 포트 · 데이터베이스 · 사용자에 맞는 줄을 `psql` 처럼 씁니다. `~/.pgpass` 는 다른 사람이 읽을 수 없을 때만 (`chmod 600 ~/.pgpass`) 읽고, 건너뛴 채 서버가 로그인을 거부하면 오류가 그 이유를 알려 줍니다. `PGSSLMODE` 는 이제 Data Source 의 TLS 설정을 바꾸지 않습니다 (Data Source 의 SSL 설정이 정합니다).
- **AWS SSM 터널** (Session Manager 포트 포워딩), SSH 대신 고를 수 있는 터널: GUI **SSH/SSL** 탭 › **Tunnel** › *AWS SSM*, TUI `Tunnel` › `aws ssm`. **AWS profile** 과 **region** (비우면 AWS CLI 기본값), **instance id** (`i-…`, 관리형 노드는 `mi-…`) 를 넣습니다. Data Source 의 Host / Port (GUI: General 탭) 는 내 컴퓨터의 포트가 아니라 그 인스턴스에서 본 데이터베이스 주소입니다. 예를 들어 bastion 뒤의 RDS 라면 Host `mydb.cluster-abc.ap-northeast-2.rds.amazonaws.com`, Port `5432` 입니다. `localhost` / `127.0.0.1` 은 인스턴스 자신이라 (`::1` 은 받지 않으니 `127.0.0.1` 을 쓰세요) 폼이 힌트로 알려 주며, 데이터베이스가 그 인스턴스에서 돌 때만 맞습니다. 인스턴스가 Host:Port 에 닿지 못하면 "AWS SSM: the instance couldn't reach …" 오류로 실패합니다. Qurry 는 `127.0.0.1` 의 임의 포트로 `aws ssm start-session --document-name AWS-StartPortForwardingSessionToRemoteHost` 를 실행하고, 플러그인이 그 포트를 열었다고 알릴 때까지 (최대 30초) 기다린 뒤 그 포트로 접속합니다. 세션 하나를 그 Data Source 의 모든 데이터베이스가 같이 씁니다. Qurry 는 자기가 연 세션을 닫습니다: 연결을 끊을 때, 세션이 제때 열리지 않았을 때, 그리고 앱을 종료하거나 앱이 비정상 종료되거나 중지될 때 AWS 쪽 세션을 끝내고 (`aws ssm terminate-session`, 같은 profile · region. 종료할 때는 한꺼번에 시작해 2초까지 기다리고, 더 걸리는 것은 혼자 끝나도록 둡니다) CLI 와 플러그인을 함께 끝냅니다. CLI 만 끝내면 세션이 인스턴스에 열린 채 남고, 열린 세션이 많은 인스턴스는 새 세션을 열지 않습니다. 그때 오류는 세션은 시작됐지만 데이터 채널이 열리지 않았다고 알려 줍니다 (그런 세션은 시간이 지나면 저절로 끝납니다). 세션을 끝내려면 내 세션에 대한 `ssm:TerminateSession` 권한이 필요합니다 (없어도 CLI 와 플러그인은 끝냅니다). 세션이 저절로 끝나면 (Session Manager 의 유휴 시간 초과, 네트워크 끊김) 다음 쿼리가 새 세션을 엽니다. instance id 가 비어 있으면 접속하지 않고 입력하라고 알려 줍니다 (호스트로 바로 접속하지 않습니다). Qurry 는 AWS 비밀을 저장하지 않습니다. AWS CLI 가 내 `~/.aws/config`, SSO 로그인, 환경 변수로 로그인합니다. [AWS CLI v2](https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html) 와 Session Manager 플러그인 (macOS: `brew install --cask session-manager-plugin`) 이 필요하고, 없으면 무엇이 없는지 오류가 알려 줍니다. SSO 세션이 만료되면 실행할 명령을 알려 줍니다 (예: `aws sso login --profile prod`). AWS 계정에는 그 인스턴스와 `AWS-StartPortForwardingSessionToRemoteHost` 문서에 대한 `ssm:StartSession` 권한이 있어야 하고, 인스턴스의 SSM 에이전트가 온라인이어야 합니다. 여러 연결을 동시에 쓰려면 플러그인 1.2.x 이상이 필요합니다. 로컬 포트에 대해서는 SSH 와 같은 주의 (같은 컴퓨터의 다른 프로그램) 가 적용됩니다. GUI 에서는 그 탭의 **Test Connection** (TUI: ⌃t) 이 세션을 열고 데이터베이스에 접속해 봅니다. 테스트하는 동안 걸린 시간을 보여 주고, **Cancel Test** (또는 Esc, TUI: ⌃t 또는 Esc) 로 테스트와 세션을 멈춥니다. Tunnel 을 다른 것으로 바꿔도 멈춥니다. 플러그인이 없을 때의 설치 링크처럼 긴 오류는 끝까지 보이도록 스크롤되어, 무엇을 해야 하는지 읽을 수 있습니다. 데이터베이스가 아직 연결 중일 때 Data Source 연결을 끊거나 저장하면, 그 데이터베이스는 "The data source was disconnected while connecting. Run it again." 을 보여 주고 닫힌 세션의 포트로 가는 연결을 남기지 않습니다. 다시 실행하면 새 세션을 엽니다. 여러 탭이나 트리가 그 Data Source 를 동시에 열다가 세션을 시작하지 못하면, 차례로 다시 시도하지 않고 모두 그 오류 하나를 보여 줍니다. 없거나 접속이 거부된 데이터베이스는 그것을 연 곳에서만 실패하고, 세션은 다른 데이터베이스를 위해 열려 있습니다. **먼저 이 컴퓨터의 Qurry 앱을 모두 업데이트하세요** (GUI, 그리고 TUI · MCP 서버는 `brew upgrade qurry` 또는 ⌃k › Update Qurry & Restart). 이전 버전은 AWS SSM 을 몰라서 호스트로 바로 접속하려 합니다. 그래서 이전 버전에는 SSM Data Source 의 저장된 비밀번호가 주어지지 않고 (비밀번호 없이 접속하다 실패합니다), 이전 버전이 Data Source 를 저장해도 업데이트된 앱에서는 SSM 설정이 그대로 남습니다.
- **SSL/TLS**: Require / Verify CA / Full Verification, CA 와 클라이언트 인증서는 선택. PostgreSQL · MySQL · Redis 에서 됩니다. SSL 을 켜면 **Require** ("encrypt, no certificate check" 로 표시) 로 시작합니다. Require 는 암호화만 하고 인증서는 확인하지 않아서, 중간에서 서버인 척 끼어드는 사람은 막지 못합니다. 이를 막는 것은 **Full Verification** (인증서와 호스트 이름 확인) 이고, 사설 CA 를 쓰는 서버라면 그 CA 파일도 함께 지정합니다 (아래 참고). Verify CA 는 인증서는 확인하지만 호스트 이름은 확인하지 않습니다. PostgreSQL · MySQL 은 시스템 신뢰 저장소가 아니라 내장된 공개 CA 목록으로 확인하므로, AWS RDS · 회사 CA · 자체 서명 인증서를 쓰는 서버는 CA 파일을 지정해야 합니다. Redis 는 시스템 신뢰 저장소를 씁니다. SSL 을 꺼 두어도 원격 PostgreSQL · MySQL 은 TLS 를 맺어야 하고 (인증서는 확인하지 않음), 못 맺으면 비밀번호를 보내지 않습니다 — 백업 · 복원도 같습니다. TLS 가 없는 서버 (또는 TLS 1.2 를 못 하는 예전 MySQL 5.7) 를 믿는 네트워크에서 쓰려면 Options 의 **Allow unencrypted connection** 을 켜세요 (TUI: 데이터 소스 폼의 `Allow unencrypted`). 그때는 Test Connection 과 상태줄에 "Not encrypted" 가 보입니다. 이 이유로 연결이 실패하면 오류에서 바로 켤 수 있습니다: 탐색기 · 콘솔 · 표 탭의 오류 아래에 **Allow unencrypted for this data source** 버튼이 나옵니다 (TUI: Data Source 이름을 보여 주고 `y` 로만 답하는 확인 창 — 입력하던 중에 뜰 수 있어서 Enter 로는 답하지 않고, 잠시 멈춘 뒤의 `y` 만 받음: 창에 키를 누를 때마다 기다림이 다시 시작됨. 다른 창이 열려 있었다면 대신 상태줄에 알리고, Data Source 를 다시 펼치거나 (`←` `→`) 다시 실행하면 묻습니다). 그 Data Source 에만 설정을 켜고 저장된 비밀번호는 그대로 둡니다. 그다음 트리와 표 탭은 다시 읽고, 콘솔은 다시 연결만 하며 ("Connected. Run again.") 문장을 다시 실행하지 않습니다. 오류가 남아 있던 트리도 다시 읽습니다. Data Source Properties 에서 Test Connection 이 실패해도 같은 버튼이 나오며, 폼의 체크박스만 켜고 다시 테스트합니다 (OK 를 눌러야 저장됩니다. TUI: `⌃t` 가 실패하면 폼의 `Allow unencrypted` 칸을 선택하고, `Space` 로 켠 뒤 `⌃t` 로 다시 테스트하며, Enter 로 저장). **Production** Data Source 에는 이 버튼이 없습니다: 메시지가 설정 위치를 알려 주고, 폼에서 직접 켭니다. 되돌리려면 **Options ▸ Allow unencrypted connection** 을 끄세요 (TUI: 데이터 소스 폼의 `Allow unencrypted`, 탐색기에서 `e`). 이 컴퓨터의 서버 (`/var/run/postgresql` 같은 PostgreSQL Unix 소켓 포함) 와 SSH · AWS SSM 터널로 가는 접속은 TLS 가 필요 없습니다. MariaDB 의 `mysql` / `mysqldump` 로 백업 · 복원할 때는 그쪽의 `--ssl` 옵션을 씁니다. 둘 다 설치돼 있으면 MySQL 클라이언트를 씁니다.
- **Options 탭**: 읽기 전용, 트랜잭션 방식과 격리 수준, 시간대, keep-alive, 오래 안 쓰면 자동 연결 끊기, 단일 데이터베이스 모드, 시작 스크립트.
- **Production**: 데이터를 바꾸는 문장을 실행하기 전에 바뀔 행 수를 세어 (같은 테이블 · WHERE 로 `SELECT count(*)`, 아무것도 바꾸지 않음) 보여 주고 묻습니다. 셀 수 없는 문장 (`UPDATE … FROM`, `DELETE … USING`, CTE) 도 묻습니다. 구조를 바꾸는 문장 (DDL) 은 "schema change" 로 보입니다. 읽기 문장은 묻지 않고 실행합니다. 쓰기 단어가 문자열이나 주석 안에만 있어도 (`select 'delete' as action`, MySQL `# delete …`) 그렇고, `"update"` 처럼 따옴표 친 이름은 여전히 묻습니다. 문자열에 백슬래시가 있으면 그 문자열이 어디서 끝나는지를 서버 설정 (MySQL `NO_BACKSLASH_ESCAPES` · `ANSI_QUOTES`, PostgreSQL `standard_conforming_strings`) 이 정하므로, 어떤 설정으로 읽어도 데이터를 바꾸는 문장이면 묻습니다. 부수 효과가 있는 함수 호출 (`dblink_exec`, `setval`, `set_config`, `pg_sleep`, MySQL `get_lock` …) 도 묻습니다. 실행하는 문자열 안에 쓰기가 숨어 있을 수 있기 때문입니다. 시퀀스 · 라지 오브젝트를 바꾸는 함수 (`nextval`, `lo_unlink` …) 와 `SELECT … INTO` (새 테이블, MySQL 서버의 파일, MySQL `@변수`) 도 묻습니다. 선택 영역을 한 문장으로 실행하면 (Query Execution Settings › For selection execute › Exactly as a single statement) 서버에 텍스트 하나로 가므로, 그 안의 문장 중 하나라도 데이터를 바꾸면 묻습니다 (`select 1; do $$ … delete … $$`, MySQL `select 1; prepare s from '…'; execute s`). GUI 와 TUI 가 같은 규칙으로 판단합니다. GUI: Options 탭 · TUI: Data Source 폼의 `Production` 토글.
- **MCP**: AI 도구가 `qurry-mcp` 로 할 수 있는 것 — Off (새 Data Source. 목록에 없고 접속하지 않음), Read, Read & write (`--allow-write` 일 때). [MCP 서버](mcp.md) 참고. GUI: Options 탭 · TUI: `MCP` 칸.

> Redis Sentinel · Cluster 노드는 자기 주소를 알려 주는 방식이라 SSH · AWS SSM 터널과 같이 쓸 수 없습니다.

## 내보내기와 가져오기

**Export Data Sources…** (탐색기 ⋮ 메뉴, Find Action) 는 모든 Data Source 를 비밀번호 없이 JSON 파일로 저장합니다. **Import Data Sources** 는 그 파일이나, DataGrip / IntelliJ IDEA · DBeaver 의 Data Source 를 읽어 옵니다:

- **GUI**: 탐색기 ⋮ › **Import Data Sources ▸** 에 이 컴퓨터에서 찾은 위치가 최신 IDE 버전부터, 들어 있는 Data Source 수와 함께 나옵니다 (경로는 툴팁). 그 아래 **File…** · **Folder…** 는 시스템 선택 창을 엽니다 (파일이나 폴더 중 하나를 고르는 창). Find Action (`⇧⌘A`) 에도 *Import Data Sources: …* 로 같은 목록이 있습니다.
- **TUI**: `⌃k` › **Import Data Sources…** 에 같은 위치가 나오고, 마지막의 **File or folder…** 는 경로를 입력받습니다 (`~` 는 홈 폴더).

형식은 고른 것으로 알아냅니다: Qurry 내보내기 파일, `dataSources.xml` · `dataSources.local.xml`, 프로젝트 폴더나 그 `.idea`, IDE options 폴더, DBeaver 의 `data-sources.json` (`data-sources-*.json`), `.dbeaver` 폴더 · DBeaver 프로젝트 · workspace 폴더. 폴더 안에 `qurry-datasources.json` 이 있으면 그 내보내기 파일을 읽습니다.

그다음 가져오기 확인 창에 Data Source 마다 한 줄씩 나옵니다: 무엇이 추가되는지 (이름, 서버), 무엇이 옮겨졌는지 (종류, SSH 터널, SSL, Read-only, Production), 오른쪽에 *password on connect*, *no password* (DataGrip 의 No auth), 또는 *SSL without certificate checks* 같은 경고. 추가되는 줄마다 체크박스가 있고 (처음엔 체크됨), 빼고 싶은 것은 체크를 풉니다. **Import N** (TUI: `y` / `Enter`) 은 체크된 것만 가져오고, 0 개면 누를 수 없습니다. TUI: `↑` `↓` 이동, `Space` 체크, `n` / `Esc` 취소. 가져올 수 없는 줄은 이유와 함께 나오고 체크박스가 없습니다: Qurry 가 지원하지 않는 종류 (Snowflake, BigQuery, Db2, CSV 파일 …), 또는 Qurry 에 이미 있는 것 (종류 · 서버 · 사용자 · 데이터베이스가 같음). 가져올 것이 하나도 없으면 확인 창 대신 그 줄들을 보여 주는 알림이 나옵니다. 읽을 수 없는 파일은 어디서 (줄 · 칸) 막혔는지 알려 주고 아무것도 가져오지 않습니다.

새 Data Source 는 새 id 를 받고 MCP Off 로 시작합니다. **비밀번호는 절대 가져오지 않습니다** (파일에 있어도): 처음 연결할 때 묻습니다. 다른 도구의 폴더에는 아무것도 쓰지 않습니다.

찾아보는 곳 (읽기만 함):

| 도구 | macOS | Linux | 읽는 파일 |
|---|---|---|---|
| DataGrip / IntelliJ (전역) | `~/Library/Application Support/JetBrains/<IDE><버전>/options` | `~/.config/JetBrains/<IDE><버전>/options` | `dataSources.xml`, `dataSources.local.xml`, `sshConfigs.xml` |
| DataGrip 프로젝트 | `<IDE 설정>/projects/<이름>/.idea`, `~/DataGripProjects/<이름>/.idea` | 같음 | `.idea/dataSources.xml`, `.idea/dataSources.local.xml`, `.idea/sshConfigs.xml` (그리고 IDE 들의 `sshConfigs.xml`) |
| 최근 IDE 프로젝트 | `<IDE 설정>/options/recentProjects.xml` 의 프로젝트 중 `.idea/dataSources.xml` 이 있는 것 | 같음 | 위와 같음 |
| DBeaver | `~/Library/DBeaverData/workspace6/<프로젝트>/.dbeaver` | `~/.local/share/DBeaverData/workspace6/…`, Flatpak `~/.var/app/io.dbeaver.DBeaverCommunity/data/DBeaverData/workspace6/…`, Snap `~/snap/dbeaver-ce/current/.local/share/DBeaverData/workspace6/…` | `data-sources.json`, `data-sources-*.json` |

옮겨지는 것:

| | DataGrip / IntelliJ | DBeaver |
|---|---|---|
| 종류, 호스트, 포트, 데이터베이스 | JDBC URL 에서: PostgreSQL, MySQL / MariaDB, SQL Server, Oracle (`@//host:port/service`. `host:port:SID` 는 SID 를 service name 으로 쓰고 경고), ClickHouse, Redis, MongoDB (`mongodb+srv://` 는 호스트로), SQLite / DuckDB (파일). URL 파라미터는 버림 | 연결의 host · port · database (없으면 URL) |
| 사용자 | `user-name` | 파일에 있으면 `configuration.user`. DBeaver 는 보통 비밀번호와 같이 (`credentials-config.json`, 읽지 않음) 두므로 그 줄에 경고가 나오고 직접 입력합니다 |
| SSH 터널 | 가리키는 SSH 설정 (`sshConfigs.xml`: 호스트, 포트, 사용자, 비밀번호 또는 키 파일), 또는 Data Source 안의 예전 터널 설정. *OpenSSH config and agent* 는 키 파일이 없어서 경고: 연결 전에 키 파일을 고르세요 | `ssh_tunnel`: 호스트, 포트, 사용자, 비밀번호 또는 키 파일 (`${project.path}` 풀어서). agent 는 경고 |
| SSL | 모드 (Require / Verify CA / Full Verification), CA, 클라이언트 인증서와 키 | `*_ssl` 핸들러: `sslMode` (disable 이면 SSL 끔), CA, 클라이언트 인증서와 키. MySQL *Verify server certificate* = Verify CA |
| Read-only | `read-only="true"` | `read-only` |
| Production | — | 연결 종류 *Production* (`prod`) |

가져오지 않는 것: 비밀번호와 SSH 암호, 드라이버 속성과 JDBC 파라미터, 스키마 필터, 색, 폴더, `~/.ssh/config` 에만 있는 SSH 호스트 이름 (터널은 DataGrip 이 저장한 호스트를 씁니다).

## 드라이버 (필요할 때 추가)

처음에는 아무것도 더 설치하지 않습니다. Data Source 종류로 아래 중 하나를 고르면 그 드라이버를 **Install** 하라고 안내합니다. 드라이버는 지금 Qurry 버전의 GitHub 릴리스에서 받는 작은 프로그램으로, 서명된 `SHA256SUMS` (앱 업데이트와 같은 키) 로 확인하고 설정 폴더의 `drivers/` 에 둡니다.

| 데이터베이스 | 참고 |
|---|---|
| SQL Server | SQL Server 2017+ (더 옛 버전도 경고와 함께 됨). `dbo` 와 다른 스키마 |
| Oracle | 무료 [Oracle Instant Client](https://www.oracle.com/database/technologies/instant-client.html) 가 필요합니다. 그 폴더를 Data Source 에 지정 (또는 `ORACLE_CLIENT_LIB_DIR`). Oracle 19c+ (11g 도 경고와 함께 됨). 서비스 이름은 Database 칸에 |
| SQLite · DuckDB | 호스트 · 포트 대신 파일 (DuckDB: 비우면 메모리) |
| ClickHouse | HTTP (포트 8123) |
| MongoDB | 콘솔에서 셸 문법: `db.users.find({age: {$gt: 30}}).sort({age: -1}).limit(10)`, `aggregate`, `countDocuments`, `insertMany`, `updateMany` … 또는 JSON 명령. 필드는 문서 50개를 보고 짐작합니다 |
| Qdrant · Chroma · Milvus | 컬렉션이 테이블입니다. 콘솔은 REST 요청을 받습니다 — [벡터 DB](vector-and-rag.md) 참고 |

연결 시험은 서버 버전을 보여 주고, Qurry 가 시험하는 버전 (PostgreSQL 12, MySQL 5.7, MariaDB 10.5, SQL Server 2017, MongoDB 5, ClickHouse 22, Oracle 19c — CI 가 이 버전들로 돕니다) 보다 낮으면 경고합니다. 읽기 전용 Data Source 는 드라이버에서도 지켜집니다: MongoDB 는 `find` / `aggregate` (`$out` / `$merge` 없이) / `count` / `distinct` 만, 벡터 DB 는 `GET` 과 검색 요청만 실행합니다.

Data Source 마다 색을 정하면 트리 배경과 탭에 표시됩니다 (운영 DB 를 구분하기 좋습니다). 정해진 색 말고도 무지개 칸을 누르면 시스템 색 고르개가 열립니다 — macOS 는 스포이드로 화면의 아무 픽셀이나 찍을 수 있어서 DataGrip 에서 쓰던 색을 그대로 가져올 수 있습니다. 옆 칸에 hex 를 직접 쳐도 됩니다. TUI 는 정해진 색을 돌려 고르고, 직접 고른 색은 그대로 둡니다.

활성 탭은 눈에 띕니다: 편집기 배경 (색을 정한 Data Source 의 탭은 그 색 그대로), 굵은 글자, 강조 막대 (창을 나눴을 때 포커스가 없는 창에서는 회색). 다른 탭은 줄무늬와 더 옅은 색을 유지합니다. 탭, Switcher, Recent Files, Search Everywhere 는 탐색기에서 그 객체 줄과 같은 아이콘을 쓰고, DDL 탭은 객체 아이콘에 작은 DDL 표시를 붙입니다 (예전 버전이 저장한 DDL 탭은 다시 열 때까지 일반 DDL 아이콘). TUI 는 탭 줄, Recent Tabs (`⌃e`), `⌃p` 검색에서 트리의 글리프를 쓰고, DDL 탭은 글리프 뒤에 `≡` 를 붙입니다. TUI 활성 탭의 모양은 [터미널에서 쓰는 TUI](tui.md#색) 를 보세요.

---

[← 사용설명서](README.md)
