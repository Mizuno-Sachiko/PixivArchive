import { apiRequest, type ApiRequest } from './client';
import { ruleCatalog } from './rule-catalog.generated';
import type { components } from './schema';

export type RuleAction = components['schemas']['RuleAction'];
export type GroupMode = components['schemas']['GroupMode'];
export type TagScope = components['schemas']['TagScope'];
export type PageQuantifier = components['schemas']['PageQuantifier'];
export type RuleField = components['schemas']['RuleField'];
export type RuleOperator = components['schemas']['RuleOperator'];
export type ConditionValue = components['schemas']['ConditionValue'];
export type RuleCondition = components['schemas']['Condition'];
export type RuleConditionGroup = components['schemas']['ConditionGroup'];
export type RuleDefinition = components['schemas']['RuleDefinitionV1'];

export type RuleDocument = RuleDefinition;

export type RuleSummary = components['schemas']['RuleDto'];
export type RuleDraft = components['schemas']['RuleDraftDto'];
export type RuleVersion = components['schemas']['RuleVersionDto'];
export type SaveRuleDraftRequest = components['schemas']['SaveRuleDraftBody'];

export type RuleTraceCondition = components['schemas']['ConditionTrace'];
export type RuleTraceGroup = components['schemas']['GroupTrace'];
export type RuleTraceEntry = components['schemas']['RuleTrace'];
export type RuleEvaluationTrace = components['schemas']['EvaluationTrace'];
export type RulePreviewItem = components['schemas']['RulePreviewItemDto'];
export type RulePreviewResponse = components['schemas']['RulePreviewResponse'];
export type PublishRuleRequest = components['schemas']['PublishRuleBody'];
export type PreviewRuleRequest = components['schemas']['PreviewRuleBody'];
export type RuleValidation = components['schemas']['RuleValidationResponse'];
export type CopyRuleRequest = components['schemas']['CopyRuleBody'];
export type ReorderRulesRequest = components['schemas']['ReorderRulesBody'];

export interface RuleWorkbenchApi {
  listRules(): Promise<RuleSummary[]>;
  createRule(name: string, defaultAction: RuleAction): Promise<RuleSummary>;
  copyRule(ruleId: string, request: CopyRuleRequest): Promise<RuleSummary>;
  reorderRules(request: ReorderRulesRequest): Promise<RuleSummary[]>;
  deleteRule(ruleId: string, expectedRevision: number): Promise<void>;
  loadDraft(ruleId: string): Promise<RuleDraft | null>;
  saveDraft(ruleId: string, request: SaveRuleDraftRequest): Promise<RuleDraft>;
  publishRule(
    ruleId: string,
    request: PublishRuleRequest
  ): Promise<RuleVersion>;
  validateRule(
    ruleId: string,
    definition: RuleDocument
  ): Promise<RuleValidation>;
  exportRule(ruleId: string): Promise<RuleDocument>;
  importRule(ruleId: string, request: SaveRuleDraftRequest): Promise<RuleDraft>;
  previewRules(
    ruleId: string,
    request: PreviewRuleRequest
  ): Promise<RulePreviewResponse>;
}

type FieldType = components['schemas']['FieldType'];
type FieldScope = components['schemas']['FieldScope'];
type RuleInitialValue = components['schemas']['RuleInitialValue'];
type RuleOperatorCatalog = components['schemas']['RuleOperatorCatalog'];
export type RuleValueOption = components['schemas']['RuleValueOption'];

export interface RuleFieldDescriptor {
  value: RuleField;
  label: string;
  category: string;
  type: FieldType;
  scope: FieldScope;
  value_example: string;
  help: string;
  options?: RuleValueOption[];
  operators: RuleOperatorCatalog[];
  case_sensitive?: boolean | null;
  tag_scope?: TagScope | null;
  page_quantifier?: PageQuantifier | null;
}

const fieldPresentation: Record<
  RuleField,
  { label: string; category: string }
> = {
  pixiv_work_id: { label: 'Pixiv作品ID', category: '作品' },
  content_type: { label: '作品类型', category: '作品' },
  title: { label: '标题', category: '作品' },
  description: { label: '简介', category: '作品' },
  tags: { label: '标签', category: '作品' },
  page_count: { label: '页数', category: '作品' },
  age_rating: { label: '年龄分级', category: '作品' },
  ai_generated: { label: 'AI生成', category: '作品' },
  original_work: { label: '原创作品', category: '作品' },
  bookmarked_by_current_account: {
    label: '当前账户已收藏',
    category: '互动'
  },
  bookmark_count: { label: '收藏数', category: '互动' },
  view_count: { label: '浏览数', category: '互动' },
  like_count: { label: '点赞数', category: '互动' },
  comment_count: { label: '评论数', category: '互动' },
  bookmark_rate: { label: '收藏率', category: '互动' },
  bookmarks_per_day: { label: '日均收藏', category: '互动' },
  ranking_rank: { label: '排行榜名次', category: '发现来源' },
  ranking_date: { label: '榜单日期', category: '发现来源' },
  artist_id: { label: '作者ID', category: '作者' },
  artist_name: { label: '作者名', category: '作者' },
  published_at: { label: '发布时间', category: '时间' },
  updated_at: { label: '更新时间', category: '时间' },
  series_id: { label: '系列ID', category: '系列' },
  series_title: { label: '系列标题', category: '系列' },
  series_order: { label: '系列顺序', category: '系列' },
  page_original_extension: { label: '原始扩展名', category: '媒体' },
  page_width: { label: '宽度', category: '媒体' },
  page_height: { label: '高度', category: '媒体' },
  page_aspect_ratio: { label: '宽高比', category: '媒体' },
  page_orientation: { label: '画面方向', category: '媒体' }
};

export const ruleFields: RuleFieldDescriptor[] = ruleCatalog.fields.map(
  (field) => {
    const presentation = fieldPresentation[field.value];
    if (!presentation) {
      throw new Error(`Missing rule field presentation: ${field.value}`);
    }
    return { ...field, ...presentation };
  }
);

export const operatorLabels: Record<RuleOperator, string> = {
  equals: '等于',
  not_equals: '不等于',
  greater_than: '大于',
  greater_than_or_equal: '大于或等于',
  less_than: '小于',
  less_than_or_equal: '小于或等于',
  between: '介于',
  not_between: '不介于',
  contains: '包含',
  not_contains: '不包含',
  starts_with: '开头是',
  ends_with: '结尾是',
  in_set: '属于列表',
  not_in_set: '不属于列表',
  in_any: '属于任一分类',
  not_in_any: '不属于这些分类',
  contains_any_tag: '包含任一标签',
  contains_all_tags: '包含全部标签',
  excludes_any_tag: '排除任一标签',
  not_contains_all_tags: '未同时包含全部标签',
  equals_tag_set: '标签集合相同',
  tag_name_contains: '标签名包含',
  tag_name_not_contains: '标签名不包含',
  count_equals: '标签数等于',
  count_greater_than_or_equal: '标签数大于或等于',
  count_less_than_or_equal: '标签数小于或等于',
  before: '早于',
  after: '晚于',
  recent_hours: '最近若干小时',
  recent_days: '最近若干天',
  is_true: '为是',
  is_false: '为否',
  exists: '有值',
  missing: '无值'
};

const ruleActionLabels = {
  download: '下载原图',
  metadata_only: '仅记录元数据',
  ignore: '完全忽略'
} satisfies Record<RuleAction, string>;

export const ruleActions: Array<{ value: RuleAction; label: string }> =
  Object.entries(ruleActionLabels).map(([value, label]) => ({
    value: value as RuleAction,
    label
  }));

export function ruleActionLabel(action: RuleAction): string {
  return ruleActionLabels[action];
}

export function operatorsForField(ruleField: RuleField): RuleOperator[] {
  return descriptorForField(ruleField).operators.map(
    (operator) => operator.value
  );
}

export function operatorDescriptorForField(
  ruleField: RuleField,
  operator: RuleOperator
): RuleOperatorCatalog {
  const descriptor = descriptorForField(ruleField).operators.find(
    (item) => item.value === operator
  );
  if (!descriptor) {
    throw new Error(`Operator ${operator} is not valid for ${ruleField}`);
  }
  return descriptor;
}

export function descriptorForField(ruleField: RuleField): RuleFieldDescriptor {
  const descriptor = ruleFields.find((item) => item.value === ruleField);
  if (!descriptor) {
    throw new Error(`Unknown rule field: ${ruleField}`);
  }
  return descriptor;
}

export function ruleConditionHelp(
  descriptor: RuleFieldDescriptor,
  requiresValue: boolean
): string {
  if (!requiresValue) return descriptor.help;
  const options = descriptor.options ?? [];
  if (options.length > 0) {
    return `${descriptor.help}；可选值：${options
      .map((option) => option.label)
      .join('、')}`;
  }
  return `${descriptor.help}；示例：${descriptor.value_example}`;
}

export function createCondition(ruleField: RuleField): RuleCondition {
  const descriptor = descriptorForField(ruleField);
  const operator = descriptor.operators[0];
  if (!operator) {
    throw new Error(`Generated rule field has no operators: ${ruleField}`);
  }
  const condition: RuleCondition = {
    field: ruleField,
    operator: operator.value
  };
  const value = materializeInitialValue(operator.initial_value);
  if (value) condition.value = value;
  if (descriptor.case_sensitive != null) {
    condition.case_sensitive = descriptor.case_sensitive;
  }
  if (descriptor.tag_scope != null) {
    condition.tag_scope = descriptor.tag_scope;
  }
  if (descriptor.page_quantifier != null) {
    condition.page_quantifier = descriptor.page_quantifier;
  }
  return condition;
}

export function valueForOperator(
  ruleField: RuleField,
  operator: RuleOperator,
  currentValue?: ConditionValue | null
): ConditionValue | undefined {
  const operatorCatalog = operatorDescriptorForField(ruleField, operator);
  const initialValue = materializeInitialValue(operatorCatalog.initial_value);
  if (!initialValue) return undefined;
  return currentValue &&
    conditionValueIsValid(ruleField, operator, currentValue)
    ? cloneConditionValue(currentValue)
    : initialValue;
}

export function conditionValueIsValid(
  ruleField: RuleField,
  operator: RuleOperator,
  value: ConditionValue
): boolean {
  const initialValue = materializeInitialValue(
    operatorDescriptorForField(ruleField, operator).initial_value
  );
  if (!initialValue || value.type !== initialValue.type) return false;

  const allowedValues = descriptorForField(ruleField).options?.map(
    (option) => option.value
  );
  if (!allowedValues?.length) return true;
  if (value.type === 'text') return allowedValues.includes(value.value);
  if (value.type === 'text_list') {
    return value.value.every((item) => allowedValues.includes(item));
  }
  return false;
}

export function createRuleDefinition(
  id: string,
  name = '新规则',
  defaultAction: RuleAction = 'download'
): RuleDefinition {
  return {
    schema_version: ruleCatalog.schema_version,
    id,
    name,
    enabled: true,
    group_mode: 'all',
    groups: [{ mode: 'all', conditions: [createCondition('bookmark_count')] }],
    action: 'download',
    default_action: defaultAction
  };
}

export function createRuleWorkbenchApi(
  request: ApiRequest = apiRequest
): RuleWorkbenchApi {
  return {
    async listRules() {
      const response =
        await request<components['schemas']['RuleList']>('/api/rules');
      return response.items;
    },
    createRule(name, defaultAction) {
      return request('/api/rules', {
        method: 'POST',
        json: { name, default_action: defaultAction }
      });
    },
    copyRule(ruleId, copyRequest) {
      return request(`/api/rules/${ruleId}/copy`, {
        method: 'POST',
        json: copyRequest
      });
    },
    async reorderRules(reorderRequest) {
      const response = await request<components['schemas']['RuleList']>(
        '/api/rules/order',
        { method: 'PUT', json: reorderRequest }
      );
      return response.items;
    },
    deleteRule(ruleId, expectedRevision) {
      return request(
        `/api/rules/${ruleId}?expected_revision=${expectedRevision}`,
        { method: 'DELETE' }
      );
    },
    loadDraft(ruleId) {
      return request(`/api/rules/${ruleId}/draft`);
    },
    saveDraft(ruleId, draft) {
      return request(`/api/rules/${ruleId}/draft`, {
        method: 'PUT',
        json: draft
      });
    },
    publishRule(ruleId, publishRequest) {
      return request(`/api/rules/${ruleId}/publish`, {
        method: 'POST',
        json: publishRequest
      });
    },
    validateRule(ruleId, definition) {
      return request(`/api/rules/${ruleId}/validate`, {
        method: 'POST',
        json: { definition }
      });
    },
    exportRule(ruleId) {
      return request(`/api/rules/${ruleId}/export`);
    },
    importRule(ruleId, draft) {
      return request(`/api/rules/${ruleId}/import`, {
        method: 'PUT',
        json: draft
      });
    },
    previewRules(ruleId, previewRequest) {
      return request(`/api/rules/${ruleId}/preview`, {
        method: 'POST',
        json: previewRequest
      });
    }
  };
}

export const ruleWorkbenchApi = createRuleWorkbenchApi();

function materializeInitialValue(
  initial: RuleInitialValue | null | undefined
): ConditionValue | undefined {
  if (!initial) return undefined;
  switch (initial.type) {
    case 'number':
    case 'duration_hours':
    case 'duration_days':
      return { type: initial.type, value: initial.value };
    case 'number_range':
      return { type: 'number_range', value: { ...initial.value } };
    case 'text':
      return { type: 'text', value: initial.value };
    case 'text_list':
      return { type: 'text_list', value: [...initial.value] };
    case 'current_date_time':
      return { type: 'date', value: new Date().toISOString() };
    case 'current_date_time_range': {
      const now = new Date().toISOString();
      return { type: 'date_range', value: { start: now, end: now } };
    }
  }
}

function cloneConditionValue(value: ConditionValue): ConditionValue {
  switch (value.type) {
    case 'number':
    case 'text':
    case 'date':
    case 'duration_hours':
    case 'duration_days':
      return { ...value };
    case 'number_range':
      return { ...value, value: { ...value.value } };
    case 'date_range':
      return { ...value, value: { ...value.value } };
    case 'text_list':
      return { ...value, value: [...value.value] };
  }
}
