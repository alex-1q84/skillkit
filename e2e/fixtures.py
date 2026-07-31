"""e2e 公共设施：等待 serve 就绪、自愈导航断言、浏览器上下文管理。"""

import time
import urllib.request
from playwright.sync_api import Page, sync_playwright

# 导航项（layout.html nav.topnav）
NAV_SEL = "nav.topnav a"
NAV_ITEMS = ["Projects", "Sources", "Skills", "Profiles"]


def wait_for_serve(base: str, timeout: float = 30.0) -> None:
    """轮询根路径 ping（公开路由，不拼 token）直到 200，serve 就绪。"""
    from urllib.parse import urlparse

    parsed = urlparse(base)
    ping_url = f"{parsed.scheme}://{parsed.netloc}/ping"
    deadline = time.time() + timeout
    last_err = None
    while time.time() < deadline:
        try:
            with urllib.request.urlopen(ping_url, timeout=2) as resp:
                if resp.status == 200:
                    return
        except Exception as e:  # noqa: BLE001
            last_err = e
        time.sleep(0.5)
    raise RuntimeError(f"serve 未在 {timeout}s 内就绪: {ping_url} ({last_err})")


def wait_until_nav_single(page: Page, timeout: float = 10.0) -> None:
    """自愈断言：nav.topnav a 恰好 1 组（防 SSE 时序导致导航重复/闪烁）。"""
    deadline = time.time() + timeout
    while time.time() < deadline:
        count = page.locator(NAV_SEL).count()
        if count == len(NAV_ITEMS):
            return
        time.sleep(0.3)
    raise AssertionError(
        f"导航数量异常：期望 {len(NAV_ITEMS)}，实际 {count}；"
        f"文本={page.locator(NAV_SEL).all_text_contents()}"
    )


def assert_nav_single(page: Page) -> None:
    """强断言：导航恰好一组且文本正确（导航重复 bug 的回归断言）。"""
    wait_until_nav_single(page)
    texts = page.locator(NAV_SEL).all_text_contents()
    assert texts == NAV_ITEMS, f"导航项不符：{texts}"


def open_page(base: str, path: str, browser) -> Page:
    """新 context + page 打开 {base}{path}。
    用 wait_until="load"（htmx 是 XHR 不阻塞 load；networkidle 会被 SSE 长连接拖死）。"""
    ctx = browser.new_context()
    page = ctx.new_page()
    page.goto(base + path, wait_until="load")
    assert_nav_single(page)
    return page


def new_browser():
    """启动 chromium，调用方 try/finally close。"""
    p = sync_playwright().start()
    browser = p.chromium.launch()
    return p, browser
