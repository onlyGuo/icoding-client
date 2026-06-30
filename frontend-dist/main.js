const invoke = window.__TAURI__?.core?.invoke;

const translations = {
  en: {
    "status.loading": "Loading status",
    "label.account": "Account",
    "label.device": "Device",
    "label.agent": "Agent",
    "label.autostart": "Autostart",
    "label.commandPermission": "Command Access",
    "label.diskPermission": "Disk Access",
    "login.title": "Sign in to your cloud account",
    "login.description": "Sign in with an email or mobile verification code. The client will register this device and connect to the cloud.",
    "login.email": "Email",
    "login.mobile": "Mobile",
    "login.emailAddress": "Email address",
    "login.mobileNumber": "Mobile number",
    "login.code": "Verification code",
    "login.sendCode": "Send code",
    "login.sending": "Sending",
    "login.codeSent": "Verification code sent.",
    "login.connect": "Sign in and connect",
    "login.connecting": "Signing in",
    "login.success": "Signed in. Connecting to the cloud.",
    "login.logout": "Log out",
    "login.loggedOut": "Logged out.",
    "client.title": "Client status",
    "client.description": "This device is signed in and can receive cloud agent tasks.",
    "client.start": "Start connection",
    "client.started": "Connection started.",
    "server.api": "API URL",
    "server.ws": "WebSocket URL",
    "server.save": "Save server URLs",
    "server.saved": "Server URLs saved.",
    "server.savedWithDraft": "Server URLs saved. New edits made during saving are still unsaved.",
    "permission.title": "Startup permission required",
    "permission.description": "The agent remains stopped until Full Disk Access is granted, preventing permission prompts during a task.",
    "permission.open": "Open Full Disk Access settings",
    "permission.waiting": "Agent has not started: {reason}",
    "permission.required": "Full Disk Access is required",
    "permission.granted": "Full Disk Access granted",
    "permission.pending": "Pending",
    "permission.notRequired": "Not required",
    "permission.instructions": "Allow iCoding Client under Full Disk Access in System Settings, then restart the app.",
    "policy.title": "Permission policy",
    "policy.roots": "Allowed directories",
    "policy.allowCommands": "Allow command execution",
    "policy.save": "Save permission policy",
    "policy.saved": "Permission policy saved.",
    "policy.savedWithDraft": "Permission policy saved. New edits made during saving are still unsaved.",
    "status.recent": "Recent status",
    "status.loggedIn": "Signed in",
    "status.loggedOut": "Signed out",
    "status.running": "Running",
    "status.stopped": "Stopped",
    "status.enabled": "Enabled",
    "status.disabled": "Disabled",
    "status.allowed": "Allowed",
    "status.denied": "Denied",
    "bridge.unavailable": "Frontend bridge unavailable",
    "bridge.error": "Tauri bridge is not available. Check withGlobalTauri in tauri.conf.json.",
  },
  zh: {
    "status.loading": "正在读取状态",
    "label.account": "账号",
    "label.device": "设备",
    "label.agent": "Agent",
    "label.autostart": "自启",
    "label.commandPermission": "命令权限",
    "label.diskPermission": "磁盘权限",
    "login.title": "登录云端账号",
    "login.description": "使用邮箱或手机号验证码登录，登录后客户端会注册当前设备并连接云端。",
    "login.email": "邮箱",
    "login.mobile": "手机",
    "login.emailAddress": "邮箱地址",
    "login.mobileNumber": "手机号",
    "login.code": "验证码",
    "login.sendCode": "发送验证码",
    "login.sending": "发送中",
    "login.codeSent": "验证码已发送。",
    "login.connect": "登录并连接",
    "login.connecting": "登录中",
    "login.success": "登录成功，正在连接云端。",
    "login.logout": "退出登录",
    "login.loggedOut": "已退出登录。",
    "client.title": "客户端状态",
    "client.description": "当前设备已登录，可以接收云端智能体任务。",
    "client.start": "启动连接",
    "client.started": "连接已启动。",
    "server.api": "API 地址",
    "server.ws": "WebSocket 地址",
    "server.save": "保存服务地址",
    "server.saved": "服务地址已保存。",
    "server.savedWithDraft": "服务地址已保存；保存期间的新修改仍未保存。",
    "permission.title": "启动权限未完成",
    "permission.description": "Agent 会在完整磁盘访问授权完成前保持停止，避免执行任务途中再次申请权限。",
    "permission.open": "打开完整磁盘访问设置",
    "permission.waiting": "Agent 尚未启动：{reason}",
    "permission.required": "需要完整磁盘访问权限",
    "permission.granted": "完整磁盘访问已授权",
    "permission.pending": "待授权",
    "permission.notRequired": "无需申请",
    "permission.instructions": "请在系统设置中允许 iCoding Client 完整磁盘访问，然后重新启动应用。",
    "policy.title": "权限策略",
    "policy.roots": "允许目录",
    "policy.allowCommands": "允许执行命令",
    "policy.save": "保存权限策略",
    "policy.saved": "权限策略已保存。",
    "policy.savedWithDraft": "权限策略已保存；保存期间的新修改仍未保存。",
    "status.recent": "最近状态",
    "status.loggedIn": "已登录",
    "status.loggedOut": "未登录",
    "status.running": "运行中",
    "status.stopped": "未运行",
    "status.enabled": "已开启",
    "status.disabled": "未开启",
    "status.allowed": "允许",
    "status.denied": "禁止",
    "bridge.unavailable": "前端桥接未就绪",
    "bridge.error": "Tauri 桥接不可用，请检查 tauri.conf.json 中的 withGlobalTauri 配置。",
  },
};

const browserLocale = (navigator.languages?.[0] || navigator.language || "en").toLowerCase();

const state = {
  loginType: "email",
  sending: false,
  serverDirty: false,
  serverRevision: 0,
  policyDirty: false,
  policyRevision: 0,
  refreshSequence: 0,
  language: browserLocale.startsWith("zh") ? "zh" : "en",
};

const $ = (id) => document.getElementById(id);

function t(key, values = {}) {
  const template = translations[state.language][key] || translations.en[key] || key;
  return Object.entries(values).reduce(
    (message, [name, value]) => message.replaceAll(`{${name}}`, String(value)),
    template,
  );
}

function applyTranslations() {
  document.documentElement.lang = state.language === "zh" ? "zh-CN" : "en";
  document.querySelectorAll("[data-i18n]").forEach((element) => {
    element.textContent = t(element.dataset.i18n);
  });
}

function showNotice(message, isError = false) {
  const notice = $("notice");
  notice.textContent = message;
  notice.classList.toggle("error", isError);
  notice.classList.remove("hidden");
}

function clearNotice() {
  $("notice").classList.add("hidden");
}

function setLoginType(type) {
  state.loginType = type;
  $("emailMode").classList.toggle("active", type === "email");
  $("mobileMode").classList.toggle("active", type === "mobile");
  $("targetLabel").textContent = type === "email" ? t("login.emailAddress") : t("login.mobileNumber");
  $("targetInput").placeholder = type === "email" ? "test@example.com" : "13800138000";
}

function targetRequest() {
  return {
    loginType: state.loginType,
    value: $("targetInput").value.trim(),
  };
}

async function refreshStatus() {
  const refreshSequence = ++state.refreshSequence;
  const status = await invoke("get_status");
  if (refreshSequence !== state.refreshSequence) {
    return;
  }

  $("connectionLabel").textContent = status.logged_in ? t("status.loggedIn") : t("status.loggedOut");
  $("accountValue").textContent =
    status.user?.email || status.user?.mobile || status.user?.nicker || "-";
  $("deviceValue").textContent = status.device_id || "-";
  $("agentValue").textContent = status.agent_running ? t("status.running") : t("status.stopped");
  $("autostartValue").textContent = status.auto_start_enabled ? t("status.enabled") : t("status.disabled");
  $("shellValue").textContent = status.policy?.shell_exec_enabled ? t("status.allowed") : t("status.denied");
  
  const diskAccessRequired = Boolean(status.permissions?.full_disk_access_required);
  const diskAccessGranted = Boolean(status.permissions?.full_disk_access_granted);
  
  $("diskAccessValue").textContent = !diskAccessRequired
    ? t("permission.notRequired")
    : diskAccessGranted
      ? t("permission.granted")
      : t("permission.pending");
  
  const shouldShowPermissionBox = diskAccessRequired && !diskAccessGranted;
  $("permissionBox").classList.toggle("hidden", !shouldShowPermissionBox);
  
  if (shouldShowPermissionBox) {
    $("permissionDetail").textContent = t("permission.waiting", {
      reason: status.permissions?.detail || t("permission.required"),
    });
  } else if (diskAccessGranted) {
    $("permissionDetail").textContent = t("permission.granted");
  } else {
    $("permissionDetail").textContent = t("permission.notRequired");
  }
  
  if (!state.serverDirty) {
    $("apiBaseUrl").value = status.server.api_base_url;
    $("wsUrl").value = status.server.ws_url;
  }
  if (!state.policyDirty) {
    $("allowedRoots").value = (status.policy?.allowed_roots || []).join("\n");
    $("shellExecEnabled").checked = Boolean(status.policy?.shell_exec_enabled);
  }
  $("statusJson").textContent = JSON.stringify(status, null, 2);

  $("loginPanel").classList.toggle("hidden", status.logged_in);
  $("statusPanel").classList.toggle("hidden", !status.logged_in);
}

async function withBusy(button, action) {
  const oldText = button.textContent;
  button.disabled = true;
  try {
    await action();
  } finally {
    button.disabled = false;
    button.textContent = oldText;
  }
}

window.addEventListener("DOMContentLoaded", () => {
  applyTranslations();
  if (!invoke) {
    $("connectionLabel").textContent = t("bridge.unavailable");
    showNotice(t("bridge.error"), true);
    return;
  }

  $("emailMode").addEventListener("click", () => setLoginType("email"));
  $("mobileMode").addEventListener("click", () => setLoginType("mobile"));
  $("apiBaseUrl").addEventListener("input", () => {
    state.serverDirty = true;
    state.serverRevision += 1;
  });
  $("wsUrl").addEventListener("input", () => {
    state.serverDirty = true;
    state.serverRevision += 1;
  });
  $("allowedRoots").addEventListener("input", () => {
    state.policyDirty = true;
    state.policyRevision += 1;
  });
  $("shellExecEnabled").addEventListener("change", () => {
    state.policyDirty = true;
    state.policyRevision += 1;
  });

  $("sendCodeBtn").addEventListener("click", async () => {
    clearNotice();
    await withBusy($("sendCodeBtn"), async () => {
      $("sendCodeBtn").textContent = t("login.sending");
      await invoke("send_code", { request: targetRequest() });
      showNotice(t("login.codeSent"));
    }).catch((error) => showNotice(String(error), true));
  });

  $("loginBtn").addEventListener("click", async () => {
    clearNotice();
    await withBusy($("loginBtn"), async () => {
      $("loginBtn").textContent = t("login.connecting");
      await invoke("verify_code", {
        request: {
          ...targetRequest(),
          code: $("codeInput").value.trim(),
        },
      });
      showNotice(t("login.success"));
      state.serverDirty = false;
      state.policyDirty = false;
      await refreshStatus();
    }).catch((error) => showNotice(String(error), true));
  });

  $("saveServerBtn").addEventListener("click", async () => {
    clearNotice();
    const revision = state.serverRevision;
    const request = {
      apiBaseUrl: $("apiBaseUrl").value.trim(),
      wsUrl: $("wsUrl").value.trim(),
    };
    await withBusy($("saveServerBtn"), async () => {
      await invoke("update_server_config", { request });
      if (state.serverRevision === revision) {
        state.serverDirty = false;
        await refreshStatus();
        showNotice(t("server.saved"));
      } else {
        showNotice(t("server.savedWithDraft"));
      }
    }).catch((error) => showNotice(String(error), true));
  });

  $("startAgentBtn").addEventListener("click", async () => {
    clearNotice();
    await invoke("start_agent")
      .then(() => showNotice(t("client.started")))
      .then(refreshStatus)
      .catch((error) => showNotice(String(error), true));
  });

  $("requestDiskAccessBtn").addEventListener("click", async () => {
    clearNotice();
    await invoke("request_full_disk_access")
      .then(() => showNotice(t("permission.instructions")))
      .catch((error) => showNotice(String(error), true));
  });

  $("savePolicyBtn").addEventListener("click", async () => {
    clearNotice();
    const revision = state.policyRevision;
    const allowedRoots = $("allowedRoots")
      .value.split("\n")
      .map((line) => line.trim())
      .filter(Boolean);

    const request = {
      allowedRoots,
      shellExecEnabled: $("shellExecEnabled").checked,
    };

    await withBusy($("savePolicyBtn"), async () => {
      await invoke("update_policy_config", { request });
      if (state.policyRevision === revision) {
        state.policyDirty = false;
        await refreshStatus();
        showNotice(t("policy.saved"));
      } else {
        showNotice(t("policy.savedWithDraft"));
      }
    }).catch((error) => showNotice(String(error), true));
  });

  $("logoutBtn").addEventListener("click", async () => {
    clearNotice();
    await invoke("logout")
      .then(() => showNotice(t("login.loggedOut")))
      .then(refreshStatus)
      .catch((error) => showNotice(String(error), true));
  });

  setLoginType("email");
  refreshStatus().catch((error) => showNotice(String(error), true));
  setInterval(() => refreshStatus().catch(() => {}), 5000);
});
