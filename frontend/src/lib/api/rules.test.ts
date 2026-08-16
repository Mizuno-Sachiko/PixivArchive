import { describe, expect, it, vi } from 'vitest';

import type { ApiRequest } from './client';
import {
  createRuleWorkbenchApi,
  conditionValueIsValid,
  descriptorForField,
  ruleActionLabel,
  ruleConditionHelp,
  ruleFields,
  valueForOperator,
  type RuleAction
} from './rules';

describe('rules API', () => {
  it('exposes only supported fields with examples and no-value operators', () => {
    const names = ruleFields.map((field) => field.value);

    expect(names).not.toContain('ranking_type');
    expect(names).not.toContain('page_sha256');
    expect(names).toContain('ranking_rank');
    expect(names).toContain('ranking_date');
    expect(ruleFields.every((field) => field.value_example && field.help)).toBe(
      true
    );
    expect(descriptorForField('title').value_example).toBe('夏日');
    expect(valueForOperator('ai_generated', 'is_true')).toBeUndefined();
  });

  it('publishes choices only for fields with a finite value range', () => {
    expect(descriptorForField('content_type')).toMatchObject({
      options: [
        { value: 'illustration', label: '插画' },
        { value: 'manga', label: '漫画' },
        { value: 'ugoira', label: '动图' }
      ]
    });
    expect(descriptorForField('age_rating')).toMatchObject({
      options: [
        { value: 'all_age', label: '全年龄' },
        { value: 'r18', label: 'R-18' },
        { value: 'r18g', label: 'R-18G' }
      ]
    });
    expect(descriptorForField('page_original_extension')).toMatchObject({
      type: 'category',
      options: [
        { value: 'jpg', label: 'JPEG' },
        { value: 'png', label: 'PNG' },
        { value: 'gif', label: 'GIF' }
      ]
    });
    expect(descriptorForField('page_orientation')).toMatchObject({
      options: [
        { value: 'portrait', label: '竖图' },
        { value: 'landscape', label: '横图' },
        { value: 'square', label: '方图' }
      ]
    });
    expect(descriptorForField('title')).not.toHaveProperty('options');
  });

  it('describes finite choices with their visible labels', () => {
    expect(ruleConditionHelp(descriptorForField('content_type'), true)).toBe(
      '作品在Pixiv中的内容类型；可选值：插画、漫画、动图'
    );
    expect(ruleConditionHelp(descriptorForField('content_type'), false)).toBe(
      '作品在Pixiv中的内容类型'
    );
  });

  it('preserves compatible condition values and resets incompatible ones', () => {
    const current = { type: 'number' as const, value: 3200 };

    const preserved = valueForOperator('bookmark_count', 'less_than', current);
    expect(preserved).toEqual(current);
    expect(preserved).not.toBe(current);
    expect(valueForOperator('bookmark_count', 'between', current)).toEqual({
      type: 'number_range',
      value: { min: 0, max: 0 }
    });
    expect(
      valueForOperator('bookmark_count', 'exists', current)
    ).toBeUndefined();
  });

  it('resets finite choices that do not belong to the selected field', () => {
    expect(
      valueForOperator('age_rating', 'equals', {
        type: 'text',
        value: 'manga'
      })
    ).toEqual({ type: 'text', value: 'all_age' });
    expect(
      valueForOperator('age_rating', 'equals', {
        type: 'text',
        value: 'r18'
      })
    ).toEqual({ type: 'text', value: 'r18' });
    expect(
      valueForOperator('age_rating', 'in_any', {
        type: 'text_list',
        value: ['r18', 'manga']
      })
    ).toEqual({ type: 'text_list', value: ['all_age'] });
    expect(
      conditionValueIsValid('age_rating', 'in_any', {
        type: 'text_list',
        value: ['all_age', 'r18g']
      })
    ).toBe(true);
  });

  it('uses one presentation label for every rule action', () => {
    expect(
      (['download', 'metadata_only', 'ignore'] as RuleAction[]).map(
        ruleActionLabel
      )
    ).toEqual(['下载原图', '仅记录元数据', '完全忽略']);
  });

  it('copies a rule under the requested name', async () => {
    const request = requestRecorder();
    const api = createRuleWorkbenchApi(request.call);

    await api.copyRule('0198f64c-42a2-7374-bace-9f1c3b317fb0', {
      name: '收藏筛选副本'
    });

    expect(request.mock).toHaveBeenCalledWith(
      '/api/rules/0198f64c-42a2-7374-bace-9f1c3b317fb0/copy',
      {
        method: 'POST',
        json: { name: '收藏筛选副本' }
      }
    );
  });

  it('persists the complete ordered rule id list', async () => {
    const request = requestRecorder();
    const api = createRuleWorkbenchApi(request.call);

    await api.reorderRules({
      ordered_rule_ids: [
        '0198f64c-42a2-7374-bace-9f1c3b317fb1',
        '0198f64c-42a2-7374-bace-9f1c3b317fb0'
      ]
    });

    expect(request.mock).toHaveBeenCalledWith('/api/rules/order', {
      method: 'PUT',
      json: {
        ordered_rule_ids: [
          '0198f64c-42a2-7374-bace-9f1c3b317fb1',
          '0198f64c-42a2-7374-bace-9f1c3b317fb0'
        ]
      }
    });
  });
});

function requestRecorder() {
  const mock = vi.fn();
  const call: ApiRequest = async <T>(...arguments_: Parameters<ApiRequest>) => {
    mock(...arguments_);
    return {} as T;
  };
  return { call, mock };
}
