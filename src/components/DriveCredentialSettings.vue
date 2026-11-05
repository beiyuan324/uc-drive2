<script setup lang="ts">
import { onActivated, reactive, ref } from 'vue';
import { NAlert, NButton, NInput, NSpace, NTabPane, NTabs, NTag, useMessage } from 'naive-ui';
import { api } from '@/api';
import { driveNames } from '@/types';
import type { DriveCredential, DrivePlatform } from '@/types';

type Platform = Exclude<DrivePlatform, 'uc'>;
const platforms: Platform[] = ['quark', 'baidu', 'xunlei'];
const active = ref<Platform>('quark');
const configured = reactive<Record<Platform, boolean>>({ quark: false, baidu: false, xunlei: false });
const inputs = reactive<Record<Platform, DriveCredential>>({ quark: {}, baidu: {}, xunlei: {} });
const busy = ref(false);
const message = useMessage();

onActivated(async () => {
  for (const platform of platforms) {
    try {
      configured[platform] = (await api.driveCredentialStatus(platform)).configured;
    } catch (e) {
      message.error((e as Error).message);
    }
  }
});

async function save(platform: Platform) {
  const credential = inputs[platform];
  if (!Object.values(credential).some((v) => v?.trim())) return message.warning('请填写要保存的凭据');
  busy.value = true;
  try {
    await api.saveDriveCredential(platform, credential);
    inputs[platform] = {};
    configured[platform] = true;
    message.success('凭据已加密保存，请重新解析分享链接');
  } catch (e) {
    message.error((e as Error).message);
  } finally {
    busy.value = false;
  }
}

async function clear(platform: Platform) {
  busy.value = true;
  try {
    await api.clearDriveCredential(platform);
    configured[platform] = false;
    inputs[platform] = {};
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
        <n-alert type="info" :show-icon="false">
          <template v-if="platform === 'quark'">从夸克官网已登录请求中复制 Cookie。仅调用夸克官方分享、目录和下载接口。</template>
          <template v-else-if="platform === 'baidu'">从百度网盘官网已登录请求中复制 Cookie。分享解析及取链均调用百度官方网页接口，下载权限和速度遵循当前账号权益；遇到验证码请先在官网完成验证。</template>
          <template v-else>从迅雷官网开发者工具中复制云盘请求的登录与校验请求头，填写下方对应值。校验令牌过期后需重新复制；普通分享无直链时，所选文件会转存到本人云盘根目录并保留。</template>
        </n-alert>
        <label>Cookie</label>
        <n-input v-model:value="inputs[platform].cookie" type="textarea" :rows="3" placeholder="仅粘贴 Cookie 的值；留空保留已保存的内容" />
        <template v-if="platform === 'xunlei'">
          <label>Authorization</label>
          <n-input v-model:value="inputs.xunlei.authorization" type="password" show-password-on="click" placeholder="官网请求头中的 Authorization（Bearer 前缀可保留）" />
          <label>X-Captcha-Token</label>
          <n-input v-model:value="inputs.xunlei.captchaToken" type="password" show-password-on="click" placeholder="官网请求头中的 X-Captcha-Token" />
          <label>X-Device-Id</label>
          <n-input v-model:value="inputs.xunlei.deviceId" placeholder="与校验令牌同一请求中的 X-Device-Id" />
          <label>X-Client-Version</label>
          <n-input v-model:value="inputs.xunlei.clientVersion" placeholder="同一请求中的 X-Client-Version（如有）" />
        </template>
        <n-space align="center">
          <n-button type="primary" secondary :loading="busy" @click="save(platform)">加密保存</n-button>
          <n-button :disabled="!configured[platform] || busy" @click="clear(platform)">清除凭据</n-button>
          <n-tag :type="configured[platform] ? 'success' : 'warning'" size="small">
            {{ configured[platform] ? '已保存凭据' : '未配置凭据' }}
          </n-tag>
        </n-space>
        <small>凭据仅加密保存在本机；空字段保留原值，清除后可完整重新填写。</small>
      </div>
    </n-tab-pane>
  </n-tabs>
</template>

<style scoped>
.credential-fields { display: flex; flex-direction: column; gap: 12px; }
.credential-fields label { font-size: 13px; color: var(--text-secondary); }
.credential-fields small { color: var(--zinc-500); }
</style>
