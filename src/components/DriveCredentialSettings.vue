<script setup lang="ts">
import { onActivated, reactive, ref } from 'vue';
import { NButton, NInput, NSpace, NTabPane, NTabs, NTag, useMessage } from 'naive-ui';
import { api } from '@/api';
import { driveNames } from '@/types';
import type { DriveCredential, DrivePlatform } from '@/types';

const platforms: DrivePlatform[] = ['uc', 'quark', 'baidu', 'xunlei'];
const active = ref<DrivePlatform>('uc');
const configured = reactive<Record<DrivePlatform, boolean>>({
  uc: false, quark: false, baidu: false, xunlei: false,
});
const inputs = reactive<Record<DrivePlatform, DriveCredential & { cookie: string }>>({
  uc: { cookie: '' }, quark: { cookie: '' }, baidu: { cookie: '' }, xunlei: { cookie: '' },
});
const busy = ref(false);
const message = useMessage();

async function refresh() {
  for (const platform of platforms) {
    try {
      configured[platform] = platform === 'uc'
        ? (await api.cookieStatus()).hasCookie
        : (await api.driveCredentialStatus(platform)).configured;
    } catch (e) {
      message.error((e as Error).message);
    }
  }
}

onActivated(refresh);
defineExpose({ refresh });

async function save(platform: DrivePlatform) {
  const credential = inputs[platform];
  if (!Object.values(credential).some((v) => v?.trim())) return message.warning('请填写要保存的凭据');
  busy.value = true;
  try {
    if (platform === 'uc') await api.saveCookie(credential.cookie.trim());
    else await api.saveDriveCredential(platform, credential);
    inputs[platform] = { cookie: '' };
    configured[platform] = true;
    message.success('凭据已保存，请重新解析分享链接');
  } catch (e) {
    message.error((e as Error).message);
  } finally {
    busy.value = false;
  }
}

async function clear(platform: DrivePlatform) {
  busy.value = true;
  try {
    if (platform === 'uc') await api.clearCookie();
    else await api.clearDriveCredential(platform);
    configured[platform] = false;
    inputs[platform] = { cookie: '' };
    message.success('凭据已清除');
  } catch (e) {
    message.error((e as Error).message);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <n-tabs v-model:value="active" type="line">
    <n-tab-pane v-for="platform in platforms" :key="platform" :name="platform" :tab="driveNames[platform]">
      <div class="credential-fields">
        <label :for="`credential-${platform}-cookie`">Cookie</label>
        <n-input
          v-model:value="inputs[platform].cookie"
          :input-props="{ id: `credential-${platform}-cookie` }"
          type="textarea"
          :rows="3"
          :placeholder="platform === 'uc' ? '粘贴官网请求头中的 Cookie' : '粘贴官网请求头中的 Cookie，留空保留原值'"
        />
        <template v-if="platform === 'xunlei'">
          <label for="credential-authorization">Authorization</label>
          <n-input v-model:value="inputs.xunlei.authorization" :input-props="{ id: 'credential-authorization' }" type="password" show-password-on="click" placeholder="官网请求头中的 Authorization，留空保留原值" />
          <label for="credential-captcha-token">X-Captcha-Token</label>
          <n-input v-model:value="inputs.xunlei.captchaToken" :input-props="{ id: 'credential-captcha-token' }" type="password" show-password-on="click" placeholder="同一请求中的 X-Captcha-Token，留空保留原值" />
          <label for="credential-device-id">X-Device-Id</label>
          <n-input v-model:value="inputs.xunlei.deviceId" :input-props="{ id: 'credential-device-id' }" placeholder="同一请求中的 X-Device-Id，留空保留原值" />
          <label for="credential-client-version">X-Client-Version</label>
          <n-input v-model:value="inputs.xunlei.clientVersion" :input-props="{ id: 'credential-client-version' }" placeholder="同一请求中的 X-Client-Version（可选），留空保留原值" />
        </template>
        <n-space align="center">
          <n-button type="primary" size="small" :loading="busy" :aria-label="`保存 ${driveNames[platform]} 凭据`" @click="save(platform)">保存</n-button>
          <n-button size="small" :disabled="!configured[platform] || busy" :aria-label="`清除 ${driveNames[platform]} 凭据`" @click="clear(platform)">清除</n-button>
          <n-tag :type="configured[platform] ? 'success' : 'default'" size="small" :bordered="false">
            {{ configured[platform] ? '已配置' : '未配置' }}
          </n-tag>
        </n-space>
      </div>
    </n-tab-pane>
  </n-tabs>
</template>

<style scoped>
.credential-fields {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.credential-fields label {
  font-size: 13px;
  color: var(--zinc-700);
}
</style>
