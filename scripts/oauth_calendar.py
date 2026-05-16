#!/usr/bin/env python3
"""Google Calendar OAuth 認證腳本 — 使用 Python 內建本地服務器完成授權"""

import json
import os
import sys
from pathlib import Path

CREDENTIALS_PATH = Path.home() / ".mcp-servers" / "google-calendar-credentials.json"
TOKEN_PATH = Path.home() / ".mcp-servers" / "google-calendar-token.json"

def main():
    if not CREDENTIALS_PATH.exists():
        print(f"❌ 找不到憑證文件: {CREDENTIALS_PATH}")
        sys.exit(1)

    print("✅ 找到憑證文件")
    print()

    from google_auth_oauthlib.flow import InstalledAppFlow

    SCOPES = ["https://www.googleapis.com/auth/calendar"]

    flow = InstalledAppFlow.from_client_secrets_file(
        str(CREDENTIALS_PATH),
        scopes=SCOPES,
    )

    print("🌐 正在啟動本地服務器並打開瀏覽器...")
    print("   請在瀏覽器中完成 Google 帳號授權")
    print()

    # run_local_server 會自動啟動本地 HTTP 服務器接收回調
    creds = flow.run_local_server(
        port=8080,
        prompt="consent",
        open_browser=True,
    )

    # 保存 token
    token_data = {
        "access_token": creds.token,
        "refresh_token": creds.refresh_token,
        "token_uri": creds.token_uri,
        "client_id": creds.client_id,
        "client_secret": creds.client_secret,
        "scopes": list(creds.scopes),
        "expiry": creds.expiry.isoformat() if creds.expiry else None,
    }

    TOKEN_PATH.parent.mkdir(parents=True, exist_ok=True)
    TOKEN_PATH.write_text(json.dumps(token_data, indent=2))

    print()
    print("✅ Token 已保存到:", TOKEN_PATH)
    print(f"   - 訪問令牌: {creds.token[:20]}...")
    print(f"   - 刷新令牌: {'✅ 有' if creds.refresh_token else '❌ 無'}")
    print()
    print("🎉 OAuth 認證完成！請重啟 VS Code 後測試日曆功能。")


if __name__ == "__main__":
    main()
