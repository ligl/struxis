# Replay K线逐条回放与结构可视化

本文档统一说明 `replay` 模块中的 K 线回放与结构可视化流程。

结构定义口径见：[struxis-core-semantics.md](struxis-core-semantics.md)

## 1) 导出结构数据（逐条回放）

```bash
cargo run -q -p replay --bin export_kline_structures -- \
  dataset/I8888.XDCE_15m.csv I8888 XDCE 15m \
  replay/web/kline-structures-data-15m.json
```

可选参数：
- `output_json`：输出 JSON 路径；
- `max_rows`：仅回放前 N 条（便于快速调试）。

示例（只回放 300 条）：

```bash
cargo run -q -p replay --bin export_kline_structures -- \
  dataset/I8888.XDCE_15m.csv I8888 XDCE 15m \
  replay/web/kline-structures-data-15m.json 300
```

## 2) 生成 1h/1d 文件

```bash
cargo run -q -p replay --bin export_kline_structures -- dataset/I8888.XDCE_60m.csv I8888 XDCE 1h replay/web/kline-structures-data-1h.json
cargo run -q -p replay --bin export_kline_structures -- dataset/I8888.XDCE_1d.csv I8888 XDCE 1d replay/web/kline-structures-data-1d.json
```

## 3) 启动本地页面

```bash
cd replay/web
python3 -m http.server 8033
```

浏览器打开：
- `http://127.0.0.1:8033/kline-structures-viewer.html?tf=15m`
- `http://127.0.0.1:8033/kline-structures-viewer.html?tf=1h`
- `http://127.0.0.1:8033/kline-structures-viewer.html?tf=1d`
