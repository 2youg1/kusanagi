#!/usr/bin/env bash
# Stage one evening in a throwaway site, then open glass on it.
#
# Four endpoints on one path host: me (the site glass opens), two friends,
# and my own agent. The friends are a group; the agent is a channel. Nothing
# here touches the real site: `LOCALAPPDATA` is pointed at a temp directory
# for every process this script starts, and glass inherits it.
#
# Usage: bash stage.sh            -> prints the stage directory, leaves glass running
set -euo pipefail
cd "$(dirname "$0")"
K="$(pwd)/zig-out/bin/kusanagi.exe"
stage=$(mktemp -d)
export LOCALAPPDATA="$stage/appdata"
mkdir -p "$LOCALAPPDATA"
host="$stage/host"
me="$K"                        # default root = $LOCALAPPDATA/kusanagi
lin="$K --root $stage/lin"
jie="$K --root $stage/jie"
agent="$K --root $stage/agent"

link() { # link <my-name-for-them> <their-cli> <their-name-for-me>
    local invite
    invite=$(printf '%s\n' "$1" | $me --json invite --name - --waypoint "$host" | grep -o 'kusanagi2:[0-9a-f]*')
    printf '%s\n%s' "$3" "$invite" | $2 join --name - > /dev/null
}
say() { printf '%s\n%s' "$2" "$3" | $1 send --to - > /dev/null; }
shout() { printf '%s\n%s' "$2" "$3" | $1 send --to-group - > /dev/null; }
hear() { printf '%s\n' "$2" | $1 read --from - > /dev/null; }

link lin "$lin" me
link jie "$jie" me
link agent "$agent" me
printf 'lin\njie\n' | $me group --name dinner > /dev/null

# The evening, in the order it happened. A read after each turn is what
# lets the next segment acknowledge it, so the thread orders itself.
shout "$me" dinner "楼下那家排到四十桌了,晚上吃什么?"
hear "$lin" me; say "$lin" me "披萨吧,不想排队"
hear "$jie" me; say "$jie" me "可以。饮料我让我的 agent 去买,你们要什么?"
hear "$me" lin; hear "$me" jie
shout "$me" dinner "定了:披萨+饮料。我让 agent 订披萨,七点楼下集合"
hear "$lin" me; say "$lin" me "好,可乐一瓶"
say "$lin" me "对了补充一句:我下午还有个会,可能会晚十分钟左右到。如果我没到你们先吃,不用等我;披萨给我留两块就行,最好是边上那种厕边厚一点的。另外楼下那家店七点后停车位很难找,你们如果开车来建议停到对面商场的 B2,从南门进比较近。"
say "$me" agent "订两份玛格丽特披萨,送到楼下前台,七点前到。预算 150 以内"
hear "$agent" me
say "$agent" me "已下单,订单如下:

- **玛格丽特披萨** × 2,中号,厚底
- 送到:楼下前台
- 预计 **18:50** 送达
- 实付 ¥128(预算 150 以内)

骑手电话我留着,到了叫你。需要改单回复 \`改\` 即可。"
hear "$me" agent
say "$me" agent "好"
hear "$agent" me
say "$agent" me "披萨到了,在楼下前台,骑手已走。下楼拿一下"
hear "$me" agent

echo "$stage"
taskkill //F //IM glass.exe > /dev/null 2>&1 || true
rm -rf .zig-cache/native-sdk-automation
(./zig-out/bin/glass.exe > .zig-cache/glass-run.log 2>&1 &)
native automate wait --timeout-ms 20000 > /dev/null
echo staged
