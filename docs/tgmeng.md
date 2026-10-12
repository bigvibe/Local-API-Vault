# tgmeng（糖果梦热榜）

本文档记录本项目接入 tgmeng 所需的**非机密**信息：接口地址、请求与返回结构、
认证方案。

**凭据不在这里。** 访问密钥存放在 Local API Vault 中，条目名为 `TGMENG_API_KEY`，
通过 `lvault get TGMENG_API_KEY` 读取。任何人都不应把密钥值写进本文档、提交到
版本库，或粘贴到聊天记录中。

注意：本文档统一称其为「密钥」而非 API token，因为它的作用是限流与机器绑定，
并非用于鉴权。

## 这是什么服务

糖果梦热榜（<https://tgmeng.com/>）是中文互联网的**热榜聚合站**，把百度、微博、
知乎、B 站、抖音、今日头条、网易、澎湃等平台的热门榜单汇总到同一套页面，并提供
RSS 订阅。

它是**内容源**，不是接口网关。它聚合的是各平台的公开榜单，数据本质上是公开信息。

## 两套并存的接口

| | RSS | 需密钥的接口 |
| --- | --- | --- |
| 认证 | 不需要 | 需群主发放的密钥 |
| 密钥作用 | 无 | 仅限流与机器绑定，**不是鉴权** |
| 稳定性 | 稳定，标准 RSS 2.0 | 待实测 |
| 本文档 | 已验证，见下 | 部分待补充 |

做原型或日常抓取，用 RSS 就够了。只有在需要更高频、更细粒度或运营特有数据时，才需要
带密钥的接口。

## RSS 接口（无需认证）

已在 2026-10-13 实测确认。

站内说明各分类与各平台都有独立 feed，路径规律为 `/<分类>/rss.xml` 与
`/<平台>/rss.xml`，完整清单见 <https://tgmeng.com/rss>。

### 核心地址

| 用途 | 地址 |
| --- | --- |
| 全站热榜 | `https://tgmeng.com/rss.xml` |
| 新闻热榜 | `https://tgmeng.com/news/rss.xml` |
| RSS 订阅总览 | `https://tgmeng.com/rss` |

面向市场情绪模块，相关性较高的分类（2026-10-13 已实测，均返回 HTTP 200、
`application/xml`、各 50 条）：

| 分类 | 地址 | 频道名 | 热度值 |
| --- | --- | --- | --- |
| 财经 | `https://tgmeng.com/finance/rss.xml` | 财经热榜 | **无** |
| 股讯 | `https://tgmeng.com/stock/rss.xml` | 股讯热榜 | **无** |
| 期货 | `https://tgmeng.com/futures/rss.xml` | 期货热榜 | **无** |

全站 `https://tgmeng.com/rss.xml`（频道名「热榜」）同样有效，各 50 条。

### 重要差异：财经类 feed 不提供热度

实测三个财经类 feed 的 `description` 全部为固定文案，**不含热度数字**：

```text
finance: [财联社] 点击标题查看详细内容
stock:   [财联社 电报] 点击标题查看详细内容
futures: [SMM 快讯] 点击标题查看详细内容
```

而 `/news/rss.xml` 等内容榜则是 `[腾讯] 热度：14824` 这种带数值的格式。

**结论：不能假设所有 feed 的 `description` 结构一致。** 如果市场情绪模块需要热度值做排序
或折线，财经/股讯/期货这三个 feed 提供不了——热度数据很可能在需要密钥的接口里。
解析器必须容忍热度缺失。

### 返回结构

标准 RSS 2.0。`channel` 层：

| 字段 | 说明 |
| --- | --- |
| `title` | 榜单名称，例如“新闻热榜 - 糖果梦热榜” |
| `link` | 对应的网页地址 |
| `description` | 榜单说明 |
| `language` | 固定 `zh-CN` |
| `lastBuildDate` | 生成时间（RFC 822） |

`item` 层：

| 字段 | 说明 |
| --- | --- |
| `title` | 热点标题 |
| `link` | 原文地址（指向来源平台，不是 tgmeng） |
| `description` | 方括号内是来源平台；内容榜形如 `[腾讯] 热度：14824`，财经榜形如 `[财联社] 点击标题查看详细内容` |
| `guid` | `<原文地址>#<标题>`，可作为去重主键 |

### 需要注意的三点

**1. 没有标准字段承载热度值。** 热度被塞在 `description` 里，需要正则提取：

```text
热度：(\d+)
```

**2. `description` 格式随 feed 变化。** 内容榜带热度（如 `[腾讯] 热度：14824`），而
财经/股讯/期货榜固定为「点击标题查看详细内容」且**完全没有热度**。热度缺失时回落到
`null`，不要让整条记录解析失败，也不要让缺失值参与排序。

**3. `link` 指向第三方平台。** 抓取时应直接使用 RSS 返回的地址，不要自行拼接。

### 最小示例

```python
import re
import urllib.request
import xml.etree.ElementTree as ET

HOT = re.compile(r"热度：(\d+)")


def fetch_hot_list(feed_url="https://tgmeng.com/news/rss.xml"):
    with urllib.request.urlopen(feed_url, timeout=10) as response:
        root = ET.fromstring(response.read())

    items = []
    for item in root.iter("item"):
        description = item.findtext("description", "")
        match = HOT.search(description)
        items.append({
            "title": item.findtext("title", ""),
            "link": item.findtext("link", ""),
            # 方括号内是来源平台；财经类 feed 形如 "[SMM 快讯] 点击标题查看详细内容"。
            "source": description.strip("[").split("]")[0],
            # 财经/股讯/期货榜没有热度，这里是 None 而非 0。
            "heat": int(match.group(1)) if match else None,
        })
    return items
```

真实响应样例（脱敏后）见 [`fixtures/tgmeng-news-rss.xml`](fixtures/tgmeng-news-rss.xml)。
若需要热度值，只能改用下面的需密钥接口。

### 使用礼仪

- 这是第三方服务，按需低频抓取。建议 5 分钟以上间隔，不要高频轮询。
- 遵守站点服务条款与各来源平台的内容版权要求。
- 榜单数据来自第三方平台，转载需自行确认授权范围。

## 密钥机制（部分接口需要）

本站为公益项目，部分功能消耗大量算力，因此引入密钥机制防止资源滥用。密钥**完全免费**，
只能通过微信交流群联系群主获取，不存在任何收费渠道。

### 密钥的用途

**密钥仅用于调用频率限制与机器绑定，不是鉴权凭据。** 它不决定「能否读到某条数据」，
只决定「这个客户端还能以多快的速度调用、能绑定几台设备」。

推论：

- 未带密钥时，被保护的接口应当因限流而被拒绝，而不是返回 401 无权限。
- 密钥不会改变返回结构，也不应改变字段含义。
- 密钥可以随时更换；更换后旧密钥失效，不影响已保存的数据。

密钥**支持多设备绑定**。这意味着它与本机环境存在绑定，迁移机器或更换网络环境时可能
失效，需要重新向群主获取。

### 与 RSS 的关系

RSS（`/rss.xml` 及各分类 feed）实测**无需密钥**即可访问。密钥机制针对的是站内需要
消耗算力的功能，两者并存。

### 调用契约（2026-10-13 实测确认）

前端 bundle（`/_nuxt/*.js`）中确认，密钥通过两个请求头传递：

| 请求头 | 内容 |
| --- | --- |
| `X-License-Code` | 密钥本体 |
| `X-Machine-Id` | 机器标识，对应「机器绑定」机制 |

端点：

```text
POST https://tgmeng.com/api/license-config
Content-Type: application/json

{
  "licenseCode": "<密钥>"
}
```

响应结构（已脱敏）：

```json
{
  "code": 200,
  "message": "success",
  "data": {
    "licenseCode": "<密钥会被原样回显，见下方警告>",
    "status": "ACTIVE",
    "isVip": false,
    "startTime": "2026-08-18T15:07:42",
    "expireTime": "2026-11-01T00:00:00",
    "limitPerMinute": 60,
    "limitPerDay": 600,
    "maxMachines": 20,
    "machineIds": ["<机器指纹>", "…"],
    "features": ["BASIC", "SEARCH", "SUBSCRIPTION", "SUDEN_NEWS", "AI_SUMMARY"],
    "subscriptionGlobalCategories": [],
    "subscriptionPlatformConfigs": [],
    "subscriptionGlobalKeywords": [],
    "id": 2188,
    "user": null,
    "remark": null
  }
}
```

同一 bundle 中还引用了 `https://trendapi.tgmeng.com` 与 `https://user.tgmeng.com`，
前者很可能是实际数据接口的域名，待确认。

> ⚠️ **安全警告：该接口会把密钥原样回显在 `data.licenseCode` 中。**
> 客户端代码**绝不能打印响应体**，也不能把它写进日志或错误信息。
> 排查问题时只应记录 `code`、`status`、`features` 等字段。

### 实测得到的账户约束

| 项 | 值 | 影响 |
| --- | --- | --- |
| 限流 | 60 次/分钟、600 次/天 | 每 5 分钟拉一次（288 次/天）在额度内 |
| 设备上限 | 20 台 | 本项目只占 1 个名额 |
| 到期时间 | **2026-11-01** | **需在到期前重新联系群主换发** |
| VIP | 否 | 部分功能可能受限 |

因为到期时间是硬约束，建议把下面这段记入 vault 条目的备注：

```text
来源：微信交流群群主发放
到期：2026-11-01
到期前需重新联系群主换发，否则功能失效
绑定：本机（占 1/20 设备名额）
```

### 待补充

- [ ] `trendapi.tgmeng.com` 上实际的数据接口路径与响应结构
- [ ] `X-Machine-Id` 的生成规则（前端如何派生，客户端能否自行指定）
- [ ] 触发限流时的错误码与响应字段
- [ ] 各 `features` 分别对应哪些端点

### 密钥保管

密钥保存在 Local API Vault 中，条目名 `TGMENG_API_KEY`。请在备注中记录获取来源与时间，
以便判断失效原因：

```text
来源：微信交流群群主发放
到期：2026-11-01
到期前需重新联系群主换发，否则功能失效
绑定：本机（占 1/20 设备名额）
```

密钥经 `X-License-Code` 请求头传递，配套 `X-Machine-Id`。详见上文调用契约。

这类密钥由人工发放，没有自助重置入口，因此比普通 API key 更值得记录来源。

## 凭据使用方式

密钥保存在 Vault 中，条目名 `TGMENG_API_KEY`，环境 `production`。调用时通过
`lvault get` 注入环境变量，**不要**写入 `.env`、命令行参数或代码：

```powershell
$env:TGMENG_API_KEY = (lvault get TGMENG_API_KEY)
```

Vault 未解锁时 `lvault get` 会返回 `Vault is locked`，此时应请用户解锁，而不是改用
明文方式绕过。

## 相关文档

- [`client-contract.md`](client-contract.md) — 本地客户端如何安全取用密钥
- [`../AGENTS.md`](../AGENTS.md) — agent 取用密钥的约定