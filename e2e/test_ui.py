"""skillkit GUI 前端 e2e（python playwright，无 pytest 纯脚本）。
用法：python test_ui.py --base http://127.0.0.1:7417/e2e-test/ --home <temp>
退出码 0 = 全过，1 = 有失败。
"""

import argparse
import sys

from playwright.sync_api import expect

from fixtures import (
    assert_nav_single,
    new_browser,
    open_page,
)

# sources 表各行 name（templates/fragments/sources_main.html）
ROWS_SEL = "table.data tbody tr"
NAME_CELL = "td:first-child"


def row_names(page):
    return [r.locator(NAME_CELL).inner_text().strip() for r in page.locator(ROWS_SEL).all()]


def expect_row(page, name, present: bool):
    """expect 轮询断言 sources 表中 {name} 行出现/消失（htmx 换页完成）。"""
    if present:
        expect(page.locator(ROWS_SEL).filter(has_text=name)).to_have_count(1)
    else:
        expect(page.locator(ROWS_SEL).filter(has_text=name)).to_have_count(0)


def test_nav_not_duplicated_after_source_delete(page, base):
    """回归：删除 source 后导航不得重复（SSE 刷新 + 写操作双通道）。"""
    # 造一个待删 source
    page.fill("form.source-add .src-package", "git@github.com:org/to-delete.git")
    page.locator("form.source-add button").click()
    expect_row(page, "to-delete", present=True)

    # 点该行删除按钮（hx-delete → 完整页 body 替换 + SSE 刷新）
    page.locator(ROWS_SEL).filter(has_text="to-delete").locator("button.x").click()
    expect_row(page, "to-delete", present=False)

    # 双通道后导航仍只有一组（bug 回归点）
    assert_nav_single(page)


def test_source_name_preview(page, base):
    """实时预览：输入 package 后 name 框显示推导名。"""
    page.fill(
        "form.source-add .src-package",
        "git@github.com:org/team-skills.git",
    )
    # htmx delay:300ms + 服务端推导；expect 轮询防竞态
    expect(page.locator("form.source-add #src-name")).to_have_value("team-skills")
    assert_nav_single(page)


def test_default_source_shown(page, base):
    """skills.sh 默认源显示（serve 启动 ensure_default 已写）。"""
    names = row_names(page)
    assert "skills.sh" in names, f"默认源未显示：{names}"
    # package 列显示 registry 搜索入口文案
    row = page.locator(ROWS_SEL).filter(has_text="skills.sh")
    assert "registry 搜索入口" in row.inner_text(), "默认源 package 列文案不符"
    assert_nav_single(page)


def test_source_add_delete_cycle(page, base):
    """增删闭环：加 source 出现、删后消失、导航仍一组。"""
    page.fill("form.source-add .src-package", "owner/cycle-repo")
    page.locator("form.source-add button").click()
    expect_row(page, "cycle-repo", present=True)

    page.locator(ROWS_SEL).filter(has_text="cycle-repo").locator("button.x").click()
    expect_row(page, "cycle-repo", present=False)
    assert_nav_single(page)


TESTS = [
    ("test_nav_not_duplicated_after_source_delete", test_nav_not_duplicated_after_source_delete),
    ("test_source_name_preview", test_source_name_preview),
    ("test_default_source_shown", test_default_source_shown),
    ("test_source_add_delete_cycle", test_source_add_delete_cycle),
]


def main():
    ap = argparse.ArgumentParser(description="skillkit GUI e2e")
    ap.add_argument("--base", required=True, help="http://127.0.0.1:PORT/TOKEN/")
    ap.add_argument("--home", required=True, help="临时 HOME（serve 隔离用）")
    ap.add_argument("--only", help="只跑指定用例名")
    args = ap.parse_args()

    from fixtures import wait_for_serve

    wait_for_serve(args.base)

    play, browser = new_browser()
    failed = []
    try:
        for name, fn in TESTS:
            if args.only and name != args.only:
                continue
            page = open_page(args.base, "sources", browser)
            try:
                fn(page, args.base)
                print(f"  ✓ {name}")
            except Exception as e:
                failed.append((name, e))
                print(f"  ✗ {name}: {e}", file=sys.stderr)
            finally:
                page.context.close()
    finally:
        browser.close()
        play.stop()

    if failed:
        print(f"\n{len(failed)} 个用例失败：", file=sys.stderr)
        for name, e in failed:
            print(f"  - {name}: {e}", file=sys.stderr)
        return 1
    print(f"\n全部 {len(TESTS)} 个用例通过")
    return 0


if __name__ == "__main__":
    sys.exit(main())
