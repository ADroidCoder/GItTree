// 命令面板用的模糊匹配：查询里的字符按顺序出现在文本里就算匹配，
// 连续匹配、匹配在单词开头、匹配在最前面的得分更高。

const BOUNDARY = /[\s/_\-.:（(]/;

/** 不匹配返回 -1，分数越高越靠前 */
export function fuzzyScore(query: string, text: string): number {
  const q = query.toLowerCase();
  const s = text.toLowerCase();
  if (!q) return 0;
  const direct = s.indexOf(q);
  if (direct >= 0) {
    // 整段包含：越靠前越好，在开头或单词开头最好
    const atWord = direct === 0 || BOUNDARY.test(s[direct - 1]);
    return 1000 - direct + (atWord ? 500 : 0) - s.length * 0.1;
  }
  let score = 0;
  let si = 0;
  let prev = -2;
  for (const ch of q) {
    const i = s.indexOf(ch, si);
    if (i < 0) return -1;
    score += 10;
    if (i === prev + 1) score += 15;
    if (i === 0 || BOUNDARY.test(s[i - 1])) score += 20;
    score -= Math.min(i - si, 10);
    prev = i;
    si = i + 1;
  }
  return score - s.length * 0.1;
}

/** 在多个文本里取最好的分数（例如命令名 + 拼音 + 英文关键词） */
export function bestScore(query: string, texts: string[]): number {
  let best = -1;
  for (const text of texts) best = Math.max(best, fuzzyScore(query, text));
  return best;
}
