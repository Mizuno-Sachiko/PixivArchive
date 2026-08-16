export const rankingModes = [
  { value: 'daily', label: '日榜' },
  { value: 'weekly', label: '周榜' },
  { value: 'monthly', label: '月榜' },
  { value: 'rookie', label: '新人榜' },
  { value: 'original', label: '原创榜' },
  { value: 'ai_generated', label: 'AI生成榜' },
  { value: 'r18', label: 'R-18榜' },
  { value: 'r18g', label: 'R-18G榜' },
  { value: 'male', label: '男性向' },
  { value: 'female', label: '女性向' }
] as const;

export const rankingContents = [
  { value: 'all', label: '综合' },
  { value: 'illustration', label: '插画' },
  { value: 'manga', label: '漫画' },
  { value: 'ugoira', label: '动图' }
] as const;

export function supportsRankingContent(mode: string, content: string): boolean {
  if (['original', 'ai_generated', 'male', 'female'].includes(mode)) {
    return content === 'all';
  }
  if (['monthly', 'rookie', 'r18g'].includes(mode)) {
    return content !== 'ugoira';
  }
  return true;
}

export function rankingCombinationCount(
  modes: string[],
  contents: string[]
): number {
  return modes.reduce(
    (count, mode) =>
      count +
      contents.filter((content) => supportsRankingContent(mode, content))
        .length,
    0
  );
}

export function rankingModeLabel(value: string): string {
  return rankingModes.find((item) => item.value === value)?.label ?? value;
}

export function rankingContentLabel(value: string): string {
  return rankingContents.find((item) => item.value === value)?.label ?? value;
}
