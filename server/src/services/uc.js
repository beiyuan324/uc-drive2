/**
 * UC 网盘解析服务：分享链接 → 会话（ctoken/stoken/cookie）→ 文件列表 → 真实下载直链。
 * 用 Node 内置 fetch 实现（esbuild 单文件打包，不引入 axios）。
 * 接口语义参考原版 uc-drive 的 uc-parser，实现完全重写。
 */

const UC_API = 'https://pc-api.uc.cn/1/clouddrive';
export const UA = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) uc-cloud-drive/2.5.20 Chrome/100.0.4896.160 Electron/18.3.5.4-b478491100 Safari/537.36 Channel/pckk_other_ch';
const PAGE_SIZE = 50;

function headersOf(cookies, ctoken = '') {
  const h = {
    'User-Agent': UA,
    'Referer': 'https://drive.uc.cn/',
    'Origin': 'https://drive.uc.cn',
    'Accept': 'application/json, text/plain, */*',
    'Cookie': cookies,
  };
  if (ctoken) h['x-csrf-token'] = ctoken;
  return h;
}

async function fetchJson(url, { method = 'GET', data, cookies, ctoken, timeout = 15000 } = {}) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeout);
  try {
    const res = await fetch(url, {
      method,
      headers: data ? { ...headersOf(cookies, ctoken), 'Content-Type': 'application/json;charset=UTF-8' } : headersOf(cookies, ctoken),
      body: data ? JSON.stringify(data) : undefined,
      redirect: 'follow',
      signal: controller.signal,
    });
    const text = await res.text();
    let json = null;
    try { json = JSON.parse(text); } catch { json = { _raw: text }; }
    return { status: res.status, json, setCookies: res.headers.getSetCookie ? res.headers.getSetCookie() : [] };
  } finally {
    clearTimeout(timer);
  }
}

/** 提取分享链接中的 shareId 与（可选的）目录 fid */
export function extractIds(shareLink) {
  const shareIdMatch = shareLink.match(/\/s\/([^?#]+)/);
  let pdirFid = null;
  const hashMatch = shareLink.match(/#\/list\/share\/[^/]*\/([a-f0-9]+)-/);
  if (hashMatch) pdirFid = hashMatch[1];
  return { shareId: shareIdMatch ? shareIdMatch[1].split(/[?#]/)[0] : null, pdirFid };
}

/** 访问分享页拿 ctoken，并合并 set-cookie 进会话 */
export async function getCtoken(shareLink, cookie = '') {
  const res = await fetch(shareLink.split('#')[0], {
    headers: { 'Cookie': cookie, 'User-Agent': UA },
    redirect: 'follow',
  });
  let ctoken = '';
  let cookies = cookie || '';
  for (const c of res.headers.getSetCookie ? res.headers.getSetCookie() : []) {
    const parts = c.split(';')[0];
    const [name, value] = parts.split('=');
    if (name === 'ctoken') ctoken = value;
    if (name && value && !cookies.includes(`${name}=`)) {
      cookies = cookies ? `${cookies}; ${parts}` : parts;
    }
  }
  return { ctoken, cookies };
}

/** 拿 stoken（分享会话令牌） */
export async function getStoken(shareId, ctoken, cookies) {
  const { json } = await fetchJson(`${UC_API}/share/sharepage/v2/detail?pr=UCBrowser&fr=pc`, {
    method: 'POST',
    data: {
      pwd_id: shareId, passcode: '', force: 0, page: 1, size: PAGE_SIZE,
      fetch_banner: 1, fetch_share: 1, fetch_total: 1,
      sort: 'file_type:asc,file_name:asc',
      banner_platform: 'other', web_platform: 'windows', fetch_error_background: 1,
    },
    cookies, ctoken,
  });
  if (!json?.data?.token_info?.stoken) {
    throw new Error(`获取 stoken 失败: ${JSON.stringify(json).slice(0, 200)}`);
  }
  return json.data.token_info.stoken;
}

/** 拉取目录列表（自动翻页直到拉完） */
export async function getFileList(shareId, stoken, pdirFid, ctoken, cookies) {
  const all = [];
  let page = 1;
  for (;;) {
    const base = `${UC_API}/share/sharepage/detail?pr=UCBrowser&fr=pc&pwd_id=${encodeURIComponent(shareId)}&stoken=${encodeURIComponent(stoken)}&force=0&_page=${page}&_size=${PAGE_SIZE}&_fetch_banner=0&_fetch_share=0&_fetch_total=1&_sort=file_type:asc,file_name:asc`;
    const url = pdirFid ? `${base}&pdir_fid=${encodeURIComponent(pdirFid)}` : base;
    const { json } = await fetchJson(url, { cookies, ctoken });
    const list = json?.data?.list;
    if (!Array.isArray(list)) {
      if (all.length === 0) throw new Error(`获取文件列表失败: ${JSON.stringify(json).slice(0, 200)}`);
      break;
    }
    all.push(...list);
    if (list.length < PAGE_SIZE) break;
    page += 1;
  }
  return all;
}

function normalizeItems(items) {
  return items.map(item => ({
    fid: item.fid,
    name: item.file_name,
    size: Number(item.size) || 0,
    file: item.file === true,
    format_type: item.format_type || '',
    share_fid_token: item.share_fid_token || '',
  }));
}

/** 递归展开全部文件（下载用） */
export async function findFiles(shareId, stoken, ctoken, cookies, pdirFid) {
  const items = await getFileList(shareId, stoken, pdirFid, ctoken, cookies);
  const files = [];
  for (const item of items) {
    if (item.file === true) files.push(item);
    else files.push(...await findFiles(shareId, stoken, ctoken, cookies, item.fid));
  }
  return files;
}

/** 单文件真实下载直链 */
export async function getDownloadUrl(shareId, stoken, fid, shareFidToken, ctoken, cookies) {
  const { json } = await fetchJson(`${UC_API}/file/download?entry=ft&fr=pc&pr=UCBrowser`, {
    method: 'POST',
    data: { fids: [fid], fids_token: [shareFidToken], pwd_id: shareId, stoken },
    cookies, ctoken,
  });
  const url = json?.data?.[0]?.download_url;
  if (!url) throw new Error(`获取下载链接失败: ${JSON.stringify(json).slice(0, 200)}`);
  return url;
}

/** 完整解析：分享链接 → 会话 + 当前目录文件列表 */
export async function parse(shareLink, cookie = '') {
  const { shareId, pdirFid } = extractIds(shareLink);
  if (!shareId) throw new Error('无法提取 share_id，请检查链接格式');
  const { ctoken, cookies } = await getCtoken(shareLink, cookie);
  const stoken = await getStoken(shareId, ctoken, cookies);
  const items = await getFileList(shareId, stoken, pdirFid, ctoken, cookies);
  if (!items.length) throw new Error('未找到文件');
  return {
    platform: 'uc',
    shareId,
    pdirFid,
    files: normalizeItems(items),
    session: { stoken, ctoken, cookies },
    shareLink,
  };
}

/** 目录浏览（保留会话） */
export async function listFolder(shareId, stoken, pdirFid, ctoken, cookies) {
  const items = await getFileList(shareId, stoken, pdirFid, ctoken, cookies);
  return normalizeItems(items);
}

/** 下载直链（保留会话），供创建任务前调用 */
export async function resolveDownload({ shareId, stoken, fid, shareFidToken, ctoken, cookies }) {
  return getDownloadUrl(shareId, stoken, fid, shareFidToken, ctoken, cookies);
}
