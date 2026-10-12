import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";

const state = {
  entries: [],
  profiles: [],
  editingProfileId: null,
  selectedId: null,
  enabled: true,
  filter: "all",
  query: "",
  secret: "",
  clipboardTimer: null,
  selectionToken: 0,
  pendingRecoveryKey: null,
  recoveryKeySaved: false,
  vaultPath: "",
};

const $ = (selector) => document.querySelector(selector);
const vaultPathInput = $("#vault-path");
const passphraseInput = $("#passphrase");
const onboarding = $("#onboarding");
const workspace = $("#workspace");
const entryList = $("#entry-list");
const entryForm = $("#entry-form");
const emptyEditor = $("#empty-editor");
const formMessage = $("#form-message");
const toast = $("#toast");
const passwordModal = $("#password-modal");
const passwordForm = $("#password-form");
const passwordModalMessage = $("#password-modal-message");
const profileModal = $("#profile-modal");
const profileForm = $("#profile-form");
const profileList = $("#profile-list");
const profileMessage = $("#profile-message");
const recoverySetupModal = $("#recovery-setup-modal");
const recoveryKeyInput = $("#recovery-key");
const recoverySetupMessage = $("#recovery-setup-message");
const recoveryAccessModal = $("#recovery-access-modal");
const recoveryForm = $("#recovery-form");
const recoveryAccessMessage = $("#recovery-access-message");

function selectedPath(value) {
  return Array.isArray(value) ? value[0] || null : value;
}

function showToast(message, tone = "info") {
  window.clearTimeout(showToast.timer);
  toast.textContent = message;
  toast.className = `toast toast-${tone}`;
  toast.hidden = false;
  showToast.timer = window.setTimeout(() => {
    toast.hidden = true;
  }, 3600);
}

function errorMessage(error) {
  if (typeof error === "string") return error;
  if (error?.message) return error.message;
  return "操作失败，请稍后重试。";
}

function setButtonBusy(button, busy, busyText = "处理中…") {
  if (busy) {
    button.dataset.originalText = button.textContent;
    button.textContent = busyText;
    button.disabled = true;
  } else {
    button.textContent = button.dataset.originalText || button.textContent;
    button.disabled = false;
  }
}

function setVaultStatus(status) {
  const statusElement = $("#vault-status");
  const lockButton = $("#lock-button");
  if (status.locked) {
    const locationElement = $("#vault-location");
    locationElement.textContent = "";
    locationElement.title = "";
    $("#enable-recovery").hidden = true;
    statusElement.textContent = "未解锁";
    statusElement.className = "status-pill locked";
    lockButton.hidden = true;
    lockButton.disabled = true;
  } else {
    const count = status.entryCount ?? 0;
    statusElement.textContent = `已解锁 · ${count} 项`;
    statusElement.className = "status-pill unlocked";
    lockButton.hidden = false;
    lockButton.disabled = false;
    const location = status.path || "";
    state.vaultPath = location;
    const locationElement = $("#vault-location");
    locationElement.textContent = location;
    locationElement.title = location;
    $("#enable-recovery").hidden = Boolean(status.recoveryEnabled);
  }
}

function showWorkspace(show) {
  workspace.hidden = !show;
  onboarding.hidden = show;
  if (!show) {
    entryForm.hidden = true;
    emptyEditor.hidden = false;
  }
}

function resetEditor() {
  state.selectedId = null;
  state.enabled = true;
  state.secret = "";
  state.selectionToken += 1;
  entryForm.reset();
  $("#entry-kind").value = "api-key";
  $("#entry-environment").value = "development";
  $("#entry-secret").type = "password";
  $("#toggle-secret").textContent = "显示";
  $("#toggle-entry-status").textContent = "停用";
  $("#editor-title").textContent = "添加密钥";
  $("#delete-entry").hidden = true;
  $("#copy-reference").hidden = true;
  formMessage.textContent = "";
  emptyEditor.hidden = true;
  entryForm.hidden = false;
}

function showEmptyEditor() {
  state.selectedId = null;
  state.secret = "";
  state.selectionToken += 1;
  entryForm.hidden = true;
  emptyEditor.hidden = false;
}

function tagsForSummary(summary) {
  return Array.isArray(summary.tags) ? summary.tags : [];
}

function matchesFilter(summary) {
  const enabled = summary.enabled !== false;
  const filterMatch =
    state.filter === "disabled"
      ? !enabled
      : state.filter === "all"
        ? enabled
        : enabled && summary.kind === state.filter;
  if (!filterMatch) return false;
  if (!state.query) return true;
  const haystack = [
    summary.name,
    summary.project,
    summary.environment,
    summary.username,
    ...tagsForSummary(summary),
  ]
    .filter(Boolean)
    .join(" ")
    .toLowerCase();
  return haystack.includes(state.query);
}

function renderEntries() {
  entryList.replaceChildren();
  const visible = state.entries.filter(matchesFilter);
  const enabledCount = state.entries.filter((entry) => entry.enabled !== false).length;
  const disabledCount = state.entries.length - enabledCount;
  $("#entry-count").textContent = disabledCount
    ? `${enabledCount} 启用 · ${disabledCount} 停用`
    : `${enabledCount} 个条目`;

  if (visible.length === 0) {
    const empty = document.createElement("div");
    empty.className = "list-empty";
    empty.textContent = state.entries.length ? "没有匹配的条目" : "还没有密钥";
    entryList.append(empty);
    return;
  }

  for (const summary of visible) {
    const item = document.createElement("button");
    item.type = "button";
    item.className = "entry-item";
    if (summary.enabled === false) item.classList.add("disabled");
    if (summary.id === state.selectedId) item.classList.add("selected");

    const icon = document.createElement("span");
    icon.className = "entry-icon";
    icon.textContent = iconForKind(summary.kind);

    const content = document.createElement("span");
    content.className = "entry-item-content";
    const title = document.createElement("span");
    title.className = "entry-item-title";
    title.textContent = summary.name;
    const meta = document.createElement("span");
    meta.className = "entry-item-meta";
    meta.textContent = [summary.project, summary.environment].filter(Boolean).join(" · ") || kindLabel(summary.kind);
    if (summary.enabled === false) meta.textContent += " · 已停用";
    content.append(title, meta);

    const arrow = document.createElement("span");
    arrow.className = "entry-arrow";
    arrow.textContent = "›";
    item.append(icon, content, arrow);
    item.addEventListener("click", () => selectEntry(summary.id));
    entryList.append(item);
  }
}

function kindLabel(kind) {
  return {
    "api-key": "API Key",
    token: "Token",
    environment: "环境变量",
    "private-key": "私钥",
    other: "其他",
  }[kind] || "密钥";
}

function iconForKind(kind) {
  return {
    "api-key": "⌁",
    token: "◈",
    environment: "{}",
    "private-key": "⌘",
    other: "•",
  }[kind] || "•";
}

async function refreshStatus() {
  const status = await invoke("vault_status");
  setVaultStatus(status);
  if (!status.locked) {
    await loadEntries();
  }
}

async function handleVaultAction(action, button) {
  const path = vaultPathInput.value.trim();
  const passphrase = passphraseInput.value;
  if (!path || !passphrase) {
    showToast("请填写 Vault 路径和主密码。", "warning");
    return;
  }
  if (
    action === "rebuild_vault" &&
    !window.confirm("重建会保留旧文件，但不会带入其中的条目。确定继续吗？")
  ) {
    return;
  }

  setButtonBusy(button, true, action === "create_vault" ? "创建中…" : action === "rebuild_vault" ? "重建中…" : "解锁中…");
  try {
    const args =
      action === "rebuild_vault"
        ? { path, newPassphrase: passphrase }
        : { path, passphrase };
    const result = await invoke(action, args);
    passphraseInput.value = "";
    showWorkspace(true);
    await loadEntries();
    await loadProfiles();
    if (result.recoveryKey) {
      openRecoverySetup(result.recoveryKey);
    }
    if (action === "rebuild_vault") {
      showToast(`旧 Vault 已保留：${result.backupPath}`, "success");
    } else {
      showToast(action === "create_vault" ? "Vault 已创建。" : "Vault 已解锁。", "success");
    }
  } catch (error) {
    showToast(errorMessage(error), "error");
  } finally {
    setButtonBusy(button, false);
  }
}

async function loadEntries() {
  state.entries = await invoke("list_entries");
  renderEntries();
  const status = await invoke("vault_status");
  setVaultStatus(status);
  if (state.selectedId && !state.entries.some((entry) => entry.id === state.selectedId)) {
    showEmptyEditor();
  }
}

async function selectEntry(id) {
  const summary = state.entries.find((entry) => entry.id === id);
  if (!summary) return;

  const token = ++state.selectionToken;
  state.selectedId = id;
  state.enabled = summary.enabled !== false;
  renderEntries();
  emptyEditor.hidden = true;
  entryForm.hidden = false;
  $("#editor-title").textContent = "编辑密钥";
  $("#toggle-entry-status").textContent = state.enabled ? "停用" : "启用";
  $("#delete-entry").hidden = false;
  $("#copy-reference").hidden = false;
  fillForm(summary);

  try {
    const revealed = await invoke("reveal_entry", { id });
    if (token !== state.selectionToken) return;
    state.secret = revealed.secret;
    $("#entry-secret").value = revealed.secret;
    $("#entry-secret").type = "password";
    $("#toggle-secret").textContent = "显示";
    formMessage.textContent = "";
  } catch (error) {
    if (token === state.selectionToken) showToast(errorMessage(error), "error");
  }
}

function fillForm(summary) {
  $("#entry-name").value = summary.name || "";
  $("#entry-kind").value = summary.kind || "api-key";
  $("#entry-project").value = summary.project || "";
  $("#entry-environment").value = summary.environment || "development";
  $("#entry-username").value = summary.username || "";
  $("#entry-url").value = summary.url || "";
  $("#entry-tags").value = tagsForSummary(summary).join(", ");
  $("#entry-notes").value = summary.notes || "";
}

async function saveEntry(event) {
  event.preventDefault();
  const secret = $("#entry-secret").value;
  if (!secret) {
    formMessage.textContent = "请输入密钥。";
    return;
  }

  const button = $("#save-entry");
  setButtonBusy(button, true, "保存中…");
  try {
    const saved = await invoke("save_entry", {
      input: {
        id: state.selectedId,
        name: $("#entry-name").value.trim(),
        kind: $("#entry-kind").value,
        project: $("#entry-project").value.trim(),
        environment: $("#entry-environment").value,
        username: $("#entry-username").value.trim(),
        secret,
        url: $("#entry-url").value.trim(),
        notes: $("#entry-notes").value.trim(),
        tags: $("#entry-tags")
          .value.split(",")
          .map((tag) => tag.trim())
          .filter(Boolean),
        enabled: state.enabled,
      },
    });
    state.selectedId = saved.id;
    state.secret = saved.secret || secret;
    await loadEntries();
    await selectEntry(saved.id);
    showToast("密钥已保存。", "success");
  } catch (error) {
    showToast(errorMessage(error), "error");
  } finally {
    setButtonBusy(button, false);
  }
}

function toggleEntryStatus() {
  state.enabled = !state.enabled;
  $("#toggle-entry-status").textContent = state.enabled ? "停用" : "启用";
  formMessage.textContent = state.enabled
    ? "保存后，此密钥将恢复为可用状态。"
    : "保存后，普通客户端将无法读取此密钥。";
}

async function deleteEntry() {
  if (!state.selectedId) return;
  if (!window.confirm("确定删除这个密钥吗？此操作不能撤销。")) return;
  const button = $("#delete-entry");
  setButtonBusy(button, true, "删除中…");
  try {
    await invoke("delete_entry", { id: state.selectedId });
    showEmptyEditor();
    await loadEntries();
    showToast("密钥已删除。", "success");
  } catch (error) {
    showToast(errorMessage(error), "error");
  } finally {
    setButtonBusy(button, false);
  }
}

async function clearClipboard() {
  try {
    await navigator.clipboard.writeText("");
  } catch {
    // 浏览器可能拒绝覆盖剪贴板；不把密钥写入日志。
  }
}

async function copySecret() {
  const value = $("#entry-secret").value;
  if (!value) {
    showToast("当前没有可复制的密钥。", "warning");
    return;
  }
  try {
    await navigator.clipboard.writeText(value);
  } catch {
    const helper = document.createElement("textarea");
    helper.value = value;
    helper.setAttribute("readonly", "");
    helper.style.position = "fixed";
    helper.style.opacity = "0";
    document.body.append(helper);
    helper.select();
    document.execCommand("copy");
    helper.remove();
  }

  window.clearTimeout(state.clipboardTimer);
  state.clipboardTimer = window.setTimeout(async () => {
    await clearClipboard();
    showToast("剪贴板已尝试清除。", "info");
  }, 30000);
  showToast("密钥已复制，将在 30 秒后清除剪贴板。", "success");
}

async function copyTextWithFallback(value) {
  try {
    await navigator.clipboard.writeText(value);
    return;
  } catch {
    // Fall through to the legacy selection-based copy.
  }
  const helper = document.createElement("textarea");
  helper.value = value;
  helper.setAttribute("readonly", "");
  helper.style.position = "fixed";
  helper.style.opacity = "0";
  document.body.append(helper);
  helper.select();
  document.execCommand("copy");
  helper.remove();
}

// Builds a share-safe reference for the selected entry. The Rust side performs
// the redaction, so the user never has to judge what is sensitive.
async function copyReference() {
  if (!state.selectedId) {
    showToast("请先选择一个密钥条目。", "warning");
    return;
  }
  const button = $("#copy-reference");
  setButtonBusy(button, true, "处理中…");
  try {
    const reference = await invoke("entry_reference", { id: state.selectedId });
    await copyTextWithFallback(reference);
    showToast("参考资料已复制（密钥已隐藏），可直接粘贴给 AI 或同事。", "success");
  } catch (error) {
    showToast(errorMessage(error), "error");
  } finally {
    setButtonBusy(button, false);
  }
}

function toggleSecret() {
  const input = $("#entry-secret");
  const visible = input.type === "text";
  input.type = visible ? "password" : "text";
  $("#toggle-secret").textContent = visible ? "显示" : "隐藏";
}

function resetProfileForm() {
  state.editingProfileId = null;
  $("#profile-id").value = "";
  $("#profile-name").value = "";
  $("#profile-directory").value = "";
  $("#profile-command").value = "";
  $("#profile-project").value = "";
  $("#profile-environment").value = "";
  populateProfileSecrets([]);
  profileMessage.textContent = "";
}

function populateProfileSecrets(selected = []) {
  const select = $("#profile-secrets");
  select.replaceChildren();
  const selectedSet = new Set(selected);
  for (const entry of state.entries) {
    const option = document.createElement("option");
    option.value = entry.name;
    option.textContent = `${entry.name}${entry.enabled === false ? "（已停用）" : ""}`;
    option.selected = selectedSet.has(entry.name);
    select.append(option);
  }
  if (!state.entries.length) {
    const option = document.createElement("option");
    option.textContent = "暂无可用密钥";
    option.disabled = true;
    select.append(option);
  }
}

function renderProfiles() {
  profileList.replaceChildren();
  if (!state.profiles.length) {
    const empty = document.createElement("div");
    empty.className = "list-empty";
    empty.textContent = "还没有运行配置";
    profileList.append(empty);
    return;
  }

  for (const profile of state.profiles) {
    const item = document.createElement("div");
    item.className = "profile-item";
    const main = document.createElement("div");
    main.className = "profile-item-main";
    const name = document.createElement("span");
    name.className = "profile-item-name";
    name.textContent = profile.name;
    const meta = document.createElement("span");
    meta.className = "profile-item-meta";
    meta.textContent = `${profile.command} · ${profile.secrets.length} 个密钥`;
    main.append(name, meta);

    const actions = document.createElement("div");
    actions.className = "profile-item-actions";
    const edit = document.createElement("button");
    edit.type = "button";
    edit.textContent = "编辑";
    edit.addEventListener("click", () => editProfile(profile));
    const copy = document.createElement("button");
    copy.type = "button";
    copy.textContent = "复制命令";
    copy.addEventListener("click", () => copyProfileCommand(profile.name));
    const remove = document.createElement("button");
    remove.type = "button";
    remove.textContent = "删除";
    remove.addEventListener("click", () => deleteProfile(profile.id, profile.name));
    actions.append(edit, copy, remove);
    item.append(main, actions);
    profileList.append(item);
  }
}

async function loadProfiles() {
  try {
    state.profiles = await invoke("list_run_profiles");
    renderProfiles();
  } catch (error) {
    showToast(errorMessage(error), "error");
  }
}

function openProfileModal() {
  resetProfileForm();
  profileModal.hidden = false;
  loadProfiles();
}

function closeProfileModal() {
  profileModal.hidden = true;
  resetProfileForm();
}

function editProfile(profile) {
  state.editingProfileId = profile.id;
  $("#profile-id").value = profile.id;
  $("#profile-name").value = profile.name;
  $("#profile-directory").value = profile.workingDirectory;
  $("#profile-command").value = profile.command;
  $("#profile-project").value = profile.project;
  $("#profile-environment").value = profile.environment;
  populateProfileSecrets(profile.secrets);
  profileMessage.textContent = "";
}

async function chooseProfileDirectory() {
  try {
    const selection = await open({ title: "选择项目目录", directory: true, multiple: false });
    const path = selectedPath(selection);
    if (path) $("#profile-directory").value = path;
  } catch (error) {
    profileMessage.textContent = errorMessage(error);
  }
}

async function copyProfileCommand(name) {
  try {
    await navigator.clipboard.writeText(`local-api-vault-cli run ${name}`);
    showToast("运行命令已复制。", "success");
  } catch {
    profileMessage.textContent = "无法访问剪贴板，请手动复制命令。";
  }
}

async function deleteProfile(id, name) {
  if (!window.confirm(`确定删除运行配置“${name}”吗？`)) return;
  try {
    await invoke("delete_run_profile", { id });
    if (state.editingProfileId === id) resetProfileForm();
    await loadProfiles();
    showToast("运行配置已删除。", "success");
  } catch (error) {
    showToast(errorMessage(error), "error");
  }
}

async function saveProfile(event) {
  event.preventDefault();
  const name = $("#profile-name").value.trim();
  const workingDirectory = $("#profile-directory").value.trim();
  const command = $("#profile-command").value.trim();
  const secrets = Array.from($("#profile-secrets").selectedOptions).map((option) => option.value);
  if (!name || !workingDirectory || !command || !secrets.length) {
    profileMessage.textContent = "名称、项目目录、启动命令和密钥名称都是必填项。";
    return;
  }

  const button = $("#save-profile");
  setButtonBusy(button, true, "保存中…");
  try {
    await invoke("save_run_profile", {
      input: {
        id: state.editingProfileId,
        name,
        workingDirectory,
        command,
        secrets,
        project: $("#profile-project").value.trim(),
        environment: $("#profile-environment").value.trim(),
      },
    });
    resetProfileForm();
    await loadProfiles();
    showToast("运行配置已保存。", "success");
  } catch (error) {
    profileMessage.textContent = errorMessage(error);
  } finally {
    setButtonBusy(button, false);
  }
}

function requestCloseRecoverySetup() {
  if (!state.recoveryKeySaved && state.pendingRecoveryKey) {
    const discard = window.confirm(
      "恢复身份文件还没有保存。关闭后将无法使用这把恢复密钥，确定关闭吗？",
    );
    if (!discard) return;
  }
  closeRecoverySetup();
}

function closeRecoverySetup() {
  recoverySetupModal.hidden = true;
  recoveryKeyInput.value = "";
  recoverySetupMessage.textContent = "";
  state.pendingRecoveryKey = null;
  state.recoveryKeySaved = false;
}

function openRecoverySetup(recoveryKey) {
  state.pendingRecoveryKey = recoveryKey;
  state.recoveryKeySaved = false;
  recoveryKeyInput.value = recoveryKey;
  recoverySetupMessage.textContent = "";
  recoverySetupModal.hidden = false;
  window.setTimeout(() => $("#save-recovery-key").focus(), 0);
}

async function copyRecoveryKey() {
  if (!state.pendingRecoveryKey) return;
  try {
    await navigator.clipboard.writeText(state.pendingRecoveryKey);
    window.clearTimeout(state.clipboardTimer);
    state.clipboardTimer = window.setTimeout(async () => {
      await clearClipboard();
      showToast("恢复密钥剪贴板已尝试清除。", "info");
    }, 30000);
    showToast("恢复密钥已复制，30 秒后会尝试清除剪贴板。", "success");
  } catch {
    recoverySetupMessage.textContent = "无法访问剪贴板，请手动选中并复制。";
  }
}

async function saveRecoveryKey() {
  if (!state.pendingRecoveryKey) return;
  try {
    const defaultPath = state.vaultPath
      ? `${state.vaultPath}.recovery-key`
      : "vault-recovery.key";
    const path = await save({
      title: "保存恢复身份文件",
      defaultPath,
      filters: [{ name: "恢复身份文件", extensions: ["key", "age", "txt"] }],
    });
    if (!path) return;
    await invoke("save_recovery_identity", {
      path,
      identity: state.pendingRecoveryKey,
    });
    state.recoveryKeySaved = true;
    recoverySetupMessage.textContent = "恢复身份文件已保存。请把主 Vault、恢复副本和身份文件分别备份。";
    showToast("恢复身份文件已保存。", "success");
  } catch (error) {
    recoverySetupMessage.textContent = errorMessage(error);
  }
}

function openRecoveryAccess() {
  $("#recovery-vault-path").value = vaultPathInput.value;
  $("#recovery-identity-path").value = "";
  $("#recovery-new-password").value = "";
  $("#recovery-confirm-password").value = "";
  recoveryAccessMessage.textContent = "";
  recoveryAccessModal.hidden = false;
  window.setTimeout(() => $("#recovery-vault-path").focus(), 0);
}

function closeRecoveryAccess() {
  recoveryAccessModal.hidden = true;
  recoveryForm.reset();
  recoveryAccessMessage.textContent = "";
}

async function chooseRecoveryIdentity() {
  try {
    const selection = await open({
      title: "选择恢复身份文件",
      multiple: false,
      directory: false,
    });
    const path = selectedPath(selection);
    if (path) $("#recovery-identity-path").value = path;
  } catch (error) {
    recoveryAccessMessage.textContent = errorMessage(error);
  }
}

async function recoverVault(event) {
  event.preventDefault();
  const path = $("#recovery-vault-path").value.trim();
  const identityPath = $("#recovery-identity-path").value.trim();
  const newPassphrase = $("#recovery-new-password").value;
  const confirmation = $("#recovery-confirm-password").value;
  if (!path || !identityPath) {
    recoveryAccessMessage.textContent = "请选择原 Vault 和恢复身份文件。";
    return;
  }
  if (newPassphrase.length < 12) {
    recoveryAccessMessage.textContent = "新主密码至少需要 12 个字符。";
    return;
  }
  if (newPassphrase !== confirmation) {
    recoveryAccessMessage.textContent = "两次输入的新主密码不一致。";
    return;
  }

  const button = $("#confirm-recovery");
  setButtonBusy(button, true, "恢复中…");
  try {
    await invoke("recover_vault", {
      path,
      identityPath,
      newPassphrase,
    });
    closeRecoveryAccess();
    vaultPathInput.value = path;
    passphraseInput.value = "";
    showWorkspace(true);
    await loadEntries();
    showToast("Vault 已恢复，新的主密码已生效。", "success");
  } catch (error) {
    recoveryAccessMessage.textContent = errorMessage(error);
  } finally {
    setButtonBusy(button, false);
    $("#recovery-new-password").value = "";
    $("#recovery-confirm-password").value = "";
  }
}

async function enableRecovery() {
  try {
    const recoveryKey = await invoke("enable_recovery");
    await loadEntries();
    openRecoverySetup(recoveryKey);
  } catch (error) {
    showToast(errorMessage(error), "error");
  }
}

function closePasswordModal() {
  passwordModal.hidden = true;
  passwordForm.reset();
  passwordModalMessage.textContent = "";
}

function openPasswordModal() {
  passwordForm.reset();
  passwordModalMessage.textContent = "";
  passwordModal.hidden = false;
  window.setTimeout(() => $("#current-password").focus(), 0);
}

async function changePassword(event) {
  event.preventDefault();
  const current = $("#current-password").value;
  const next = $("#new-password").value;
  const confirmation = $("#confirm-password").value;
  if (next.length < 12) {
    passwordModalMessage.textContent = "新主密码至少需要 12 个字符。";
    return;
  }
  if (next !== confirmation) {
    passwordModalMessage.textContent = "两次输入的新主密码不一致。";
    return;
  }

  const button = $("#confirm-password-change");
  setButtonBusy(button, true, "更新中…");
  try {
    await invoke("change_passphrase", {
      oldPassphrase: current,
      newPassphrase: next,
    });
    closePasswordModal();
    showToast("主密码已更新。", "success");
  } catch (error) {
    passwordModalMessage.textContent = errorMessage(error);
  } finally {
    setButtonBusy(button, false);
    $("#current-password").value = "";
    $("#new-password").value = "";
    $("#confirm-password").value = "";
  }
}

async function chooseVault() {
  try {
    const selection = await open({
      title: "选择 Vault 文件",
      multiple: false,
      directory: false,
      filters: [{ name: "Vault", extensions: ["age", "vault"] }],
    });
    const path = selectedPath(selection);
    if (path) vaultPathInput.value = path;
  } catch (error) {
    showToast(errorMessage(error), "error");
  }
}

async function importEnv() {
  try {
    const selection = await open({
      title: "选择 .env 文件",
      multiple: false,
      directory: false,
    });
    const path = selectedPath(selection);
    if (!path) return;
    const result = await invoke("import_env", { path });
    await loadEntries();
    const skipped = result.skippedCount ? `，跳过 ${result.skippedCount} 个重复项` : "";
    showToast(`已导入 ${result.importedCount} 个环境变量${skipped}。`, "success");
  } catch (error) {
    showToast(errorMessage(error), "error");
  }
}

async function exportBackup() {
  try {
    const status = await invoke("vault_status");
    if (status.locked || !status.path) {
      showToast("请先解锁 Vault。", "warning");
      return;
    }
    const source = status.path.replace(/\.age$/i, "");
    const destination = await save({
      title: "保存加密备份",
      defaultPath: `${source}-backup.age`,
      filters: [{ name: "Vault 备份", extensions: ["age"] }],
    });
    if (!destination) return;
    await invoke("export_backup", { destination });
    showToast("加密备份已保存。", "success");
  } catch (error) {
    showToast(errorMessage(error), "error");
  }
}

async function lockVault(alreadyLocked = false) {
  try {
    if (!alreadyLocked) await invoke("lock_vault");
    window.clearTimeout(state.clipboardTimer);
    await clearClipboard();
    state.entries = [];
    state.selectedId = null;
    state.secret = "";
    passphraseInput.value = "";
    $("#entry-secret").value = "";
    showWorkspace(false);
    closeProfileModal();
    closePasswordModal();
    closeRecoverySetup();
    closeRecoveryAccess();
    setVaultStatus({ locked: true, entryCount: 0, recoveryEnabled: false });
    showToast("Vault 已锁定。", "success");
  } catch (error) {
    showToast(errorMessage(error), "error");
  }
}

function updateAutostartButton(enabled) {
  const button = $("#toggle-autostart");
  if (!button) return;
  button.textContent = enabled ? "开机自启动：开" : "开机自启动：关";
  button.classList.toggle("active", enabled);
}

async function loadAutostart() {
  try {
    updateAutostartButton(await invoke("autostart_status"));
  } catch {
    // 开机自启动目前仅支持 Windows，失败时保持默认关闭。
  }
}

async function toggleAutostart() {
  const button = $("#toggle-autostart");
  setButtonBusy(button, true, "处理中…");
  try {
    const current = await invoke("autostart_status");
    await invoke("set_autostart", { enabled: !current });
    showToast(
      !current
        ? "已开启开机自启动：登录后静默进入托盘，且保持锁定。"
        : "已关闭开机自启动。",
      "success",
    );
  } catch (error) {
    showToast(errorMessage(error), "error");
  } finally {
    setButtonBusy(button, false);
    await loadAutostart();
  }
}

function bindEvents() {
  $("#create-vault").addEventListener("click", (event) => handleVaultAction("create_vault", event.currentTarget));
  $("#rebuild-vault").addEventListener("click", (event) => handleVaultAction("rebuild_vault", event.currentTarget));
  $("#recover-vault").addEventListener("click", openRecoveryAccess);
  $("#open-vault").addEventListener("click", (event) => handleVaultAction("open_vault", event.currentTarget));
  $("#lock-button").addEventListener("click", () => lockVault());
  $("#add-entry").addEventListener("click", resetEditor);
  $("#delete-entry").addEventListener("click", deleteEntry);
  $("#toggle-entry-status").addEventListener("click", toggleEntryStatus);
  $("#choose-vault").addEventListener("click", chooseVault);
  $("#import-env").addEventListener("click", importEnv);
  $("#export-backup").addEventListener("click", exportBackup);
  $("#change-password").addEventListener("click", openPasswordModal);
  $("#run-profiles").addEventListener("click", openProfileModal);
  $("#toggle-autostart").addEventListener("click", toggleAutostart);
  $("#close-profile-modal").addEventListener("click", closeProfileModal);
  $("#clear-profile-form").addEventListener("click", resetProfileForm);
  $("#choose-profile-directory").addEventListener("click", chooseProfileDirectory);
  profileForm.addEventListener("submit", saveProfile);
  $("#enable-recovery").addEventListener("click", enableRecovery);
  $("#close-password-modal").addEventListener("click", closePasswordModal);
  $("#cancel-password").addEventListener("click", closePasswordModal);
  $("#close-recovery-setup").addEventListener("click", requestCloseRecoverySetup);
  $("#finish-recovery-setup").addEventListener("click", requestCloseRecoverySetup);
  $("#copy-recovery-key").addEventListener("click", copyRecoveryKey);
  $("#save-recovery-key").addEventListener("click", saveRecoveryKey);
  $("#close-recovery-access").addEventListener("click", closeRecoveryAccess);
  $("#cancel-recovery").addEventListener("click", closeRecoveryAccess);
  $("#choose-recovery-identity").addEventListener("click", chooseRecoveryIdentity);
  recoveryForm.addEventListener("submit", recoverVault);
  passwordForm.addEventListener("submit", changePassword);
  $("#copy-secret").addEventListener("click", copySecret);
  $("#copy-reference").addEventListener("click", copyReference);
  $("#toggle-secret").addEventListener("click", toggleSecret);
  entryForm.addEventListener("submit", saveEntry);

  $("#search-input").addEventListener("input", (event) => {
    state.query = event.target.value.trim().toLowerCase();
    renderEntries();
  });

  document.querySelectorAll("[data-filter]").forEach((button) => {
    button.addEventListener("click", () => {
      document.querySelectorAll("[data-filter]").forEach((item) => item.classList.remove("active"));
      button.classList.add("active");
      state.filter = button.dataset.filter;
      renderEntries();
    });
  });
}

async function boot() {
  bindEvents();
  try {
    vaultPathInput.value = await invoke("default_vault_path");
    await refreshStatus();
    await loadAutostart();
  } catch (error) {
    showToast(errorMessage(error), "error");
  }
}

boot();
