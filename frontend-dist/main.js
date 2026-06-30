const invoke = window.__TAURI__?.core?.invoke;

const state = {
  loginType: "email",
  sending: false,
  serverDirty: false,
  serverRevision: 0,
  policyDirty: false,
  policyRevision: 0,
  refreshSequence: 0,
};

const $ = (id) => document.getElementById(id);

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
  $("targetLabel").textContent = type === "email" ? "邮箱地址" : "手机号";
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

  $("connectionLabel").textContent = status.logged_in ? "已登录" : "未登录";
  $("accountValue").textContent =
    status.user?.email || status.user?.mobile || status.user?.nicker || "-";
  $("deviceValue").textContent = status.device_id || "-";
  $("agentValue").textContent = status.agent_running ? "运行中" : "未运行";
  $("autostartValue").textContent = status.auto_start_enabled ? "已开启" : "未开启";
  $("shellValue").textContent = status.policy?.shell_exec_enabled ? "允许" : "禁止";
  
  const diskAccessRequired = Boolean(status.permissions?.full_disk_access_required);
  const diskAccessGranted = Boolean(status.permissions?.full_disk_access_granted);
  
  // 更新磁盘权限状态显示
  $("diskAccessValue").textContent = !diskAccessRequired
    ? "无需申请"
    : diskAccessGranted
      ? "已授权"
      : "待授权";
  
  // 处理权限提示框的显示逻辑
  const shouldShowPermissionBox = diskAccessRequired && !diskAccessGranted;
  $("permissionBox").classList.toggle("hidden", !shouldShowPermissionBox);
  
  // 更新权限详情文本
  if (shouldShowPermissionBox) {
    $("permissionDetail").textContent = `Agent 尚未启动：${status.permissions?.detail || "需要完整磁盘访问权限"}`;
  } else if (diskAccessGranted) {
    // 权限已授权，但提示框应该被隐藏，这里设置一个默认值
    $("permissionDetail").textContent = "完整磁盘访问已授权。";
  } else {
    // 不需要权限的情况
    $("permissionDetail").textContent = "无需申请完整磁盘访问权限。";
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
  if (!invoke) {
    $("connectionLabel").textContent = "前端桥接未就绪";
    showNotice("Tauri bridge is not available. Check withGlobalTauri in tauri.conf.json.", true);
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
      $("sendCodeBtn").textContent = "发送中";
      await invoke("send_code", { request: targetRequest() });
      showNotice("验证码已发送");
    }).catch((error) => showNotice(String(error), true));
  });

  $("loginBtn").addEventListener("click", async () => {
    clearNotice();
    await withBusy($("loginBtn"), async () => {
      $("loginBtn").textContent = "登录中";
      await invoke("verify_code", {
        request: {
          ...targetRequest(),
          code: $("codeInput").value.trim(),
        },
      });
      showNotice("登录成功，正在连接云端");
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
        showNotice("服务地址已保存");
      } else {
        showNotice("服务地址已保存；保存期间的新修改仍未保存");
      }
    }).catch((error) => showNotice(String(error), true));
  });

  $("startAgentBtn").addEventListener("click", async () => {
    clearNotice();
    await invoke("start_agent")
      .then(() => showNotice("连接已启动"))
      .then(refreshStatus)
      .catch((error) => showNotice(String(error), true));
  });

  $("requestDiskAccessBtn").addEventListener("click", async () => {
    clearNotice();
    await invoke("request_full_disk_access")
      .then(() => showNotice("请在系统设置中允许 iCoding Client 完整磁盘访问，然后重新启动应用"))
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
        showNotice("权限策略已保存");
      } else {
        showNotice("权限策略已保存；保存期间的新修改仍未保存");
      }
    }).catch((error) => showNotice(String(error), true));
  });

  $("logoutBtn").addEventListener("click", async () => {
    clearNotice();
    await invoke("logout")
      .then(() => showNotice("已退出登录"))
      .then(refreshStatus)
      .catch((error) => showNotice(String(error), true));
  });

  setLoginType("email");
  refreshStatus().catch((error) => showNotice(String(error), true));
  setInterval(() => refreshStatus().catch(() => {}), 5000);
});
