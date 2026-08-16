import type { components } from './schema';

export const ruleCatalog: components['schemas']['RuleCatalog'] = {
  "schema_version": 1,
  "fields": [
    {
      "value": "pixiv_work_id",
      "type": "number",
      "scope": "work",
      "value_example": "123456789",
      "help": "Pixiv作品的数字ID",
      "operators": [
        {
          "value": "equals",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "not_equals",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "greater_than",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "greater_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "less_than",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "less_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "between",
          "requires_value": true,
          "initial_value": {
            "type": "number_range",
            "value": {
              "min": 0.0,
              "max": 0.0
            }
          }
        },
        {
          "value": "not_between",
          "requires_value": true,
          "initial_value": {
            "type": "number_range",
            "value": {
              "min": 0.0,
              "max": 0.0
            }
          }
        },
        {
          "value": "exists",
          "requires_value": false
        },
        {
          "value": "missing",
          "requires_value": false
        }
      ]
    },
    {
      "value": "content_type",
      "type": "category",
      "scope": "work",
      "value_example": "illustration",
      "help": "作品在Pixiv中的内容类型",
      "options": [
        {
          "value": "illustration",
          "label": "插画"
        },
        {
          "value": "manga",
          "label": "漫画"
        },
        {
          "value": "ugoira",
          "label": "动图"
        }
      ],
      "operators": [
        {
          "value": "equals",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": "illustration"
          }
        },
        {
          "value": "not_equals",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": "illustration"
          }
        },
        {
          "value": "in_any",
          "requires_value": true,
          "initial_value": {
            "type": "text_list",
            "value": [
              "illustration"
            ]
          }
        },
        {
          "value": "not_in_any",
          "requires_value": true,
          "initial_value": {
            "type": "text_list",
            "value": [
              "illustration"
            ]
          }
        },
        {
          "value": "exists",
          "requires_value": false
        },
        {
          "value": "missing",
          "requires_value": false
        }
      ]
    },
    {
      "value": "title",
      "type": "text",
      "scope": "work",
      "value_example": "夏日",
      "help": "作品标题",
      "operators": [
        {
          "value": "equals",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": ""
          }
        },
        {
          "value": "not_equals",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": ""
          }
        },
        {
          "value": "contains",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": ""
          }
        },
        {
          "value": "not_contains",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": ""
          }
        },
        {
          "value": "starts_with",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": ""
          }
        },
        {
          "value": "ends_with",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": ""
          }
        },
        {
          "value": "in_set",
          "requires_value": true,
          "initial_value": {
            "type": "text_list",
            "value": [
              ""
            ]
          }
        },
        {
          "value": "not_in_set",
          "requires_value": true,
          "initial_value": {
            "type": "text_list",
            "value": [
              ""
            ]
          }
        },
        {
          "value": "exists",
          "requires_value": false
        },
        {
          "value": "missing",
          "requires_value": false
        }
      ],
      "case_sensitive": false
    },
    {
      "value": "description",
      "type": "text",
      "scope": "work",
      "value_example": "海边",
      "help": "作品简介中的文本",
      "operators": [
        {
          "value": "equals",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": ""
          }
        },
        {
          "value": "not_equals",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": ""
          }
        },
        {
          "value": "contains",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": ""
          }
        },
        {
          "value": "not_contains",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": ""
          }
        },
        {
          "value": "starts_with",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": ""
          }
        },
        {
          "value": "ends_with",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": ""
          }
        },
        {
          "value": "in_set",
          "requires_value": true,
          "initial_value": {
            "type": "text_list",
            "value": [
              ""
            ]
          }
        },
        {
          "value": "not_in_set",
          "requires_value": true,
          "initial_value": {
            "type": "text_list",
            "value": [
              ""
            ]
          }
        },
        {
          "value": "exists",
          "requires_value": false
        },
        {
          "value": "missing",
          "requires_value": false
        }
      ],
      "case_sensitive": false
    },
    {
      "value": "artist_id",
      "type": "number",
      "scope": "work",
      "value_example": "12345",
      "help": "Pixiv作者的数字ID",
      "operators": [
        {
          "value": "equals",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "not_equals",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "greater_than",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "greater_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "less_than",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "less_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "between",
          "requires_value": true,
          "initial_value": {
            "type": "number_range",
            "value": {
              "min": 0.0,
              "max": 0.0
            }
          }
        },
        {
          "value": "not_between",
          "requires_value": true,
          "initial_value": {
            "type": "number_range",
            "value": {
              "min": 0.0,
              "max": 0.0
            }
          }
        },
        {
          "value": "exists",
          "requires_value": false
        },
        {
          "value": "missing",
          "requires_value": false
        }
      ]
    },
    {
      "value": "artist_name",
      "type": "text",
      "scope": "work",
      "value_example": "作者名",
      "help": "作者显示名称",
      "operators": [
        {
          "value": "equals",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": ""
          }
        },
        {
          "value": "not_equals",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": ""
          }
        },
        {
          "value": "contains",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": ""
          }
        },
        {
          "value": "not_contains",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": ""
          }
        },
        {
          "value": "starts_with",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": ""
          }
        },
        {
          "value": "ends_with",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": ""
          }
        },
        {
          "value": "in_set",
          "requires_value": true,
          "initial_value": {
            "type": "text_list",
            "value": [
              ""
            ]
          }
        },
        {
          "value": "not_in_set",
          "requires_value": true,
          "initial_value": {
            "type": "text_list",
            "value": [
              ""
            ]
          }
        },
        {
          "value": "exists",
          "requires_value": false
        },
        {
          "value": "missing",
          "requires_value": false
        }
      ],
      "case_sensitive": false
    },
    {
      "value": "published_at",
      "type": "date",
      "scope": "work",
      "value_example": "2026-08-01T12:30:00Z",
      "help": "作品在Pixiv的发布时间",
      "operators": [
        {
          "value": "before",
          "requires_value": true,
          "initial_value": {
            "type": "current_date_time"
          }
        },
        {
          "value": "after",
          "requires_value": true,
          "initial_value": {
            "type": "current_date_time"
          }
        },
        {
          "value": "between",
          "requires_value": true,
          "initial_value": {
            "type": "current_date_time_range"
          }
        },
        {
          "value": "recent_hours",
          "requires_value": true,
          "initial_value": {
            "type": "duration_hours",
            "value": 24
          }
        },
        {
          "value": "recent_days",
          "requires_value": true,
          "initial_value": {
            "type": "duration_days",
            "value": 7
          }
        },
        {
          "value": "exists",
          "requires_value": false
        },
        {
          "value": "missing",
          "requires_value": false
        }
      ]
    },
    {
      "value": "updated_at",
      "type": "date",
      "scope": "work",
      "value_example": "2026-08-01T12:30:00Z",
      "help": "作品元数据的最近更新时间",
      "operators": [
        {
          "value": "before",
          "requires_value": true,
          "initial_value": {
            "type": "current_date_time"
          }
        },
        {
          "value": "after",
          "requires_value": true,
          "initial_value": {
            "type": "current_date_time"
          }
        },
        {
          "value": "between",
          "requires_value": true,
          "initial_value": {
            "type": "current_date_time_range"
          }
        },
        {
          "value": "recent_hours",
          "requires_value": true,
          "initial_value": {
            "type": "duration_hours",
            "value": 24
          }
        },
        {
          "value": "recent_days",
          "requires_value": true,
          "initial_value": {
            "type": "duration_days",
            "value": 7
          }
        },
        {
          "value": "exists",
          "requires_value": false
        },
        {
          "value": "missing",
          "requires_value": false
        }
      ]
    },
    {
      "value": "tags",
      "type": "tags",
      "scope": "work",
      "value_example": "猫, original",
      "help": "多个标签使用逗号分隔",
      "operators": [
        {
          "value": "contains_any_tag",
          "requires_value": true,
          "initial_value": {
            "type": "text_list",
            "value": [
              ""
            ]
          }
        },
        {
          "value": "contains_all_tags",
          "requires_value": true,
          "initial_value": {
            "type": "text_list",
            "value": [
              ""
            ]
          }
        },
        {
          "value": "excludes_any_tag",
          "requires_value": true,
          "initial_value": {
            "type": "text_list",
            "value": [
              ""
            ]
          }
        },
        {
          "value": "not_contains_all_tags",
          "requires_value": true,
          "initial_value": {
            "type": "text_list",
            "value": [
              ""
            ]
          }
        },
        {
          "value": "equals_tag_set",
          "requires_value": true,
          "initial_value": {
            "type": "text_list",
            "value": [
              ""
            ]
          }
        },
        {
          "value": "tag_name_contains",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": ""
          }
        },
        {
          "value": "tag_name_not_contains",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": ""
          }
        },
        {
          "value": "count_equals",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "count_greater_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "count_less_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "exists",
          "requires_value": false
        },
        {
          "value": "missing",
          "requires_value": false
        }
      ],
      "case_sensitive": false,
      "tag_scope": "original_and_translation"
    },
    {
      "value": "page_count",
      "type": "number",
      "scope": "work",
      "value_example": "2",
      "help": "作品包含的页面数量",
      "operators": [
        {
          "value": "equals",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "not_equals",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "greater_than",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "greater_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "less_than",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "less_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "between",
          "requires_value": true,
          "initial_value": {
            "type": "number_range",
            "value": {
              "min": 0.0,
              "max": 0.0
            }
          }
        },
        {
          "value": "not_between",
          "requires_value": true,
          "initial_value": {
            "type": "number_range",
            "value": {
              "min": 0.0,
              "max": 0.0
            }
          }
        },
        {
          "value": "exists",
          "requires_value": false
        },
        {
          "value": "missing",
          "requires_value": false
        }
      ]
    },
    {
      "value": "age_rating",
      "type": "category",
      "scope": "work",
      "value_example": "all_age",
      "help": "Pixiv标注的年龄分级",
      "options": [
        {
          "value": "all_age",
          "label": "全年龄"
        },
        {
          "value": "r18",
          "label": "R-18"
        },
        {
          "value": "r18g",
          "label": "R-18G"
        }
      ],
      "operators": [
        {
          "value": "equals",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": "all_age"
          }
        },
        {
          "value": "not_equals",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": "all_age"
          }
        },
        {
          "value": "in_any",
          "requires_value": true,
          "initial_value": {
            "type": "text_list",
            "value": [
              "all_age"
            ]
          }
        },
        {
          "value": "not_in_any",
          "requires_value": true,
          "initial_value": {
            "type": "text_list",
            "value": [
              "all_age"
            ]
          }
        },
        {
          "value": "exists",
          "requires_value": false
        },
        {
          "value": "missing",
          "requires_value": false
        }
      ]
    },
    {
      "value": "ai_generated",
      "type": "boolean",
      "scope": "work",
      "value_example": "true",
      "help": "Pixiv标记的AI生成状态",
      "operators": [
        {
          "value": "is_true",
          "requires_value": false
        },
        {
          "value": "is_false",
          "requires_value": false
        },
        {
          "value": "exists",
          "requires_value": false
        },
        {
          "value": "missing",
          "requires_value": false
        }
      ]
    },
    {
      "value": "original_work",
      "type": "boolean",
      "scope": "work",
      "value_example": "true",
      "help": "作品是否标记为原创",
      "operators": [
        {
          "value": "is_true",
          "requires_value": false
        },
        {
          "value": "is_false",
          "requires_value": false
        },
        {
          "value": "exists",
          "requires_value": false
        },
        {
          "value": "missing",
          "requires_value": false
        }
      ]
    },
    {
      "value": "bookmarked_by_current_account",
      "type": "boolean",
      "scope": "work",
      "value_example": "true",
      "help": "当前Pixiv账户是否已收藏",
      "operators": [
        {
          "value": "is_true",
          "requires_value": false
        },
        {
          "value": "is_false",
          "requires_value": false
        },
        {
          "value": "exists",
          "requires_value": false
        },
        {
          "value": "missing",
          "requires_value": false
        }
      ]
    },
    {
      "value": "bookmark_count",
      "type": "number",
      "scope": "work",
      "value_example": "1000",
      "help": "Pixiv收藏数量",
      "operators": [
        {
          "value": "equals",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "not_equals",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "greater_than",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "greater_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "less_than",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "less_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "between",
          "requires_value": true,
          "initial_value": {
            "type": "number_range",
            "value": {
              "min": 0.0,
              "max": 0.0
            }
          }
        },
        {
          "value": "not_between",
          "requires_value": true,
          "initial_value": {
            "type": "number_range",
            "value": {
              "min": 0.0,
              "max": 0.0
            }
          }
        },
        {
          "value": "exists",
          "requires_value": false
        },
        {
          "value": "missing",
          "requires_value": false
        }
      ]
    },
    {
      "value": "view_count",
      "type": "number",
      "scope": "work",
      "value_example": "5000",
      "help": "Pixiv浏览数量",
      "operators": [
        {
          "value": "equals",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "not_equals",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "greater_than",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "greater_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "less_than",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "less_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "between",
          "requires_value": true,
          "initial_value": {
            "type": "number_range",
            "value": {
              "min": 0.0,
              "max": 0.0
            }
          }
        },
        {
          "value": "not_between",
          "requires_value": true,
          "initial_value": {
            "type": "number_range",
            "value": {
              "min": 0.0,
              "max": 0.0
            }
          }
        },
        {
          "value": "exists",
          "requires_value": false
        },
        {
          "value": "missing",
          "requires_value": false
        }
      ]
    },
    {
      "value": "like_count",
      "type": "number",
      "scope": "work",
      "value_example": "300",
      "help": "Pixiv点赞数量",
      "operators": [
        {
          "value": "equals",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "not_equals",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "greater_than",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "greater_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "less_than",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "less_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "between",
          "requires_value": true,
          "initial_value": {
            "type": "number_range",
            "value": {
              "min": 0.0,
              "max": 0.0
            }
          }
        },
        {
          "value": "not_between",
          "requires_value": true,
          "initial_value": {
            "type": "number_range",
            "value": {
              "min": 0.0,
              "max": 0.0
            }
          }
        },
        {
          "value": "exists",
          "requires_value": false
        },
        {
          "value": "missing",
          "requires_value": false
        }
      ]
    },
    {
      "value": "comment_count",
      "type": "number",
      "scope": "work",
      "value_example": "20",
      "help": "Pixiv评论数量",
      "operators": [
        {
          "value": "equals",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "not_equals",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "greater_than",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "greater_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "less_than",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "less_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "between",
          "requires_value": true,
          "initial_value": {
            "type": "number_range",
            "value": {
              "min": 0.0,
              "max": 0.0
            }
          }
        },
        {
          "value": "not_between",
          "requires_value": true,
          "initial_value": {
            "type": "number_range",
            "value": {
              "min": 0.0,
              "max": 0.0
            }
          }
        },
        {
          "value": "exists",
          "requires_value": false
        },
        {
          "value": "missing",
          "requires_value": false
        }
      ]
    },
    {
      "value": "bookmark_rate",
      "type": "number",
      "scope": "work",
      "value_example": "0.2",
      "help": "收藏数除以浏览数",
      "operators": [
        {
          "value": "equals",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "not_equals",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "greater_than",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "greater_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "less_than",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "less_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "between",
          "requires_value": true,
          "initial_value": {
            "type": "number_range",
            "value": {
              "min": 0.0,
              "max": 0.0
            }
          }
        },
        {
          "value": "not_between",
          "requires_value": true,
          "initial_value": {
            "type": "number_range",
            "value": {
              "min": 0.0,
              "max": 0.0
            }
          }
        },
        {
          "value": "exists",
          "requires_value": false
        },
        {
          "value": "missing",
          "requires_value": false
        }
      ]
    },
    {
      "value": "bookmarks_per_day",
      "type": "number",
      "scope": "work",
      "value_example": "10",
      "help": "发布后平均每天新增的收藏数",
      "operators": [
        {
          "value": "equals",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "not_equals",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "greater_than",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "greater_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "less_than",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "less_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "between",
          "requires_value": true,
          "initial_value": {
            "type": "number_range",
            "value": {
              "min": 0.0,
              "max": 0.0
            }
          }
        },
        {
          "value": "not_between",
          "requires_value": true,
          "initial_value": {
            "type": "number_range",
            "value": {
              "min": 0.0,
              "max": 0.0
            }
          }
        },
        {
          "value": "exists",
          "requires_value": false
        },
        {
          "value": "missing",
          "requires_value": false
        }
      ]
    },
    {
      "value": "ranking_rank",
      "type": "number",
      "scope": "work",
      "value_example": "1",
      "help": "作品在榜单中的名次",
      "operators": [
        {
          "value": "equals",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "not_equals",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "greater_than",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "greater_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "less_than",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "less_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "between",
          "requires_value": true,
          "initial_value": {
            "type": "number_range",
            "value": {
              "min": 0.0,
              "max": 0.0
            }
          }
        },
        {
          "value": "not_between",
          "requires_value": true,
          "initial_value": {
            "type": "number_range",
            "value": {
              "min": 0.0,
              "max": 0.0
            }
          }
        },
        {
          "value": "exists",
          "requires_value": false
        },
        {
          "value": "missing",
          "requires_value": false
        }
      ]
    },
    {
      "value": "ranking_date",
      "type": "date",
      "scope": "work",
      "value_example": "2026-08-01",
      "help": "榜单日期",
      "operators": [
        {
          "value": "before",
          "requires_value": true,
          "initial_value": {
            "type": "current_date_time"
          }
        },
        {
          "value": "after",
          "requires_value": true,
          "initial_value": {
            "type": "current_date_time"
          }
        },
        {
          "value": "between",
          "requires_value": true,
          "initial_value": {
            "type": "current_date_time_range"
          }
        },
        {
          "value": "recent_hours",
          "requires_value": true,
          "initial_value": {
            "type": "duration_hours",
            "value": 24
          }
        },
        {
          "value": "recent_days",
          "requires_value": true,
          "initial_value": {
            "type": "duration_days",
            "value": 7
          }
        },
        {
          "value": "exists",
          "requires_value": false
        },
        {
          "value": "missing",
          "requires_value": false
        }
      ]
    },
    {
      "value": "series_id",
      "type": "number",
      "scope": "work",
      "value_example": "456",
      "help": "Pixiv系列的数字ID",
      "operators": [
        {
          "value": "equals",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "not_equals",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "greater_than",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "greater_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "less_than",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "less_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "between",
          "requires_value": true,
          "initial_value": {
            "type": "number_range",
            "value": {
              "min": 0.0,
              "max": 0.0
            }
          }
        },
        {
          "value": "not_between",
          "requires_value": true,
          "initial_value": {
            "type": "number_range",
            "value": {
              "min": 0.0,
              "max": 0.0
            }
          }
        },
        {
          "value": "exists",
          "requires_value": false
        },
        {
          "value": "missing",
          "requires_value": false
        }
      ]
    },
    {
      "value": "series_title",
      "type": "text",
      "scope": "work",
      "value_example": "系列名",
      "help": "Pixiv系列标题",
      "operators": [
        {
          "value": "equals",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": ""
          }
        },
        {
          "value": "not_equals",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": ""
          }
        },
        {
          "value": "contains",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": ""
          }
        },
        {
          "value": "not_contains",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": ""
          }
        },
        {
          "value": "starts_with",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": ""
          }
        },
        {
          "value": "ends_with",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": ""
          }
        },
        {
          "value": "in_set",
          "requires_value": true,
          "initial_value": {
            "type": "text_list",
            "value": [
              ""
            ]
          }
        },
        {
          "value": "not_in_set",
          "requires_value": true,
          "initial_value": {
            "type": "text_list",
            "value": [
              ""
            ]
          }
        },
        {
          "value": "exists",
          "requires_value": false
        },
        {
          "value": "missing",
          "requires_value": false
        }
      ],
      "case_sensitive": false
    },
    {
      "value": "series_order",
      "type": "number",
      "scope": "work",
      "value_example": "1",
      "help": "作品在系列中的顺序",
      "operators": [
        {
          "value": "equals",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "not_equals",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "greater_than",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "greater_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "less_than",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "less_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "between",
          "requires_value": true,
          "initial_value": {
            "type": "number_range",
            "value": {
              "min": 0.0,
              "max": 0.0
            }
          }
        },
        {
          "value": "not_between",
          "requires_value": true,
          "initial_value": {
            "type": "number_range",
            "value": {
              "min": 0.0,
              "max": 0.0
            }
          }
        },
        {
          "value": "exists",
          "requires_value": false
        },
        {
          "value": "missing",
          "requires_value": false
        }
      ]
    },
    {
      "value": "page_original_extension",
      "type": "category",
      "scope": "page",
      "value_example": "png",
      "help": "原图文件扩展名",
      "options": [
        {
          "value": "jpg",
          "label": "JPEG"
        },
        {
          "value": "png",
          "label": "PNG"
        },
        {
          "value": "gif",
          "label": "GIF"
        }
      ],
      "operators": [
        {
          "value": "equals",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": "jpg"
          }
        },
        {
          "value": "not_equals",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": "jpg"
          }
        },
        {
          "value": "in_any",
          "requires_value": true,
          "initial_value": {
            "type": "text_list",
            "value": [
              "jpg"
            ]
          }
        },
        {
          "value": "not_in_any",
          "requires_value": true,
          "initial_value": {
            "type": "text_list",
            "value": [
              "jpg"
            ]
          }
        },
        {
          "value": "exists",
          "requires_value": false
        },
        {
          "value": "missing",
          "requires_value": false
        }
      ],
      "page_quantifier": "any_page"
    },
    {
      "value": "page_width",
      "type": "number",
      "scope": "page",
      "value_example": "2048",
      "help": "页面宽度，单位为像素",
      "operators": [
        {
          "value": "equals",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "not_equals",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "greater_than",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "greater_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "less_than",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "less_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "between",
          "requires_value": true,
          "initial_value": {
            "type": "number_range",
            "value": {
              "min": 0.0,
              "max": 0.0
            }
          }
        },
        {
          "value": "not_between",
          "requires_value": true,
          "initial_value": {
            "type": "number_range",
            "value": {
              "min": 0.0,
              "max": 0.0
            }
          }
        },
        {
          "value": "exists",
          "requires_value": false
        },
        {
          "value": "missing",
          "requires_value": false
        }
      ],
      "page_quantifier": "any_page"
    },
    {
      "value": "page_height",
      "type": "number",
      "scope": "page",
      "value_example": "3072",
      "help": "页面高度，单位为像素",
      "operators": [
        {
          "value": "equals",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "not_equals",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "greater_than",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "greater_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "less_than",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "less_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "between",
          "requires_value": true,
          "initial_value": {
            "type": "number_range",
            "value": {
              "min": 0.0,
              "max": 0.0
            }
          }
        },
        {
          "value": "not_between",
          "requires_value": true,
          "initial_value": {
            "type": "number_range",
            "value": {
              "min": 0.0,
              "max": 0.0
            }
          }
        },
        {
          "value": "exists",
          "requires_value": false
        },
        {
          "value": "missing",
          "requires_value": false
        }
      ],
      "page_quantifier": "any_page"
    },
    {
      "value": "page_aspect_ratio",
      "type": "number",
      "scope": "page",
      "value_example": "0.6667",
      "help": "页面宽度除以高度",
      "operators": [
        {
          "value": "equals",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "not_equals",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "greater_than",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "greater_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "less_than",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "less_than_or_equal",
          "requires_value": true,
          "initial_value": {
            "type": "number",
            "value": 0.0
          }
        },
        {
          "value": "between",
          "requires_value": true,
          "initial_value": {
            "type": "number_range",
            "value": {
              "min": 0.0,
              "max": 0.0
            }
          }
        },
        {
          "value": "not_between",
          "requires_value": true,
          "initial_value": {
            "type": "number_range",
            "value": {
              "min": 0.0,
              "max": 0.0
            }
          }
        },
        {
          "value": "exists",
          "requires_value": false
        },
        {
          "value": "missing",
          "requires_value": false
        }
      ],
      "page_quantifier": "any_page"
    },
    {
      "value": "page_orientation",
      "type": "category",
      "scope": "page",
      "value_example": "portrait",
      "help": "根据页面宽高计算的画面方向",
      "options": [
        {
          "value": "portrait",
          "label": "竖图"
        },
        {
          "value": "landscape",
          "label": "横图"
        },
        {
          "value": "square",
          "label": "方图"
        }
      ],
      "operators": [
        {
          "value": "equals",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": "portrait"
          }
        },
        {
          "value": "not_equals",
          "requires_value": true,
          "initial_value": {
            "type": "text",
            "value": "portrait"
          }
        },
        {
          "value": "in_any",
          "requires_value": true,
          "initial_value": {
            "type": "text_list",
            "value": [
              "portrait"
            ]
          }
        },
        {
          "value": "not_in_any",
          "requires_value": true,
          "initial_value": {
            "type": "text_list",
            "value": [
              "portrait"
            ]
          }
        },
        {
          "value": "exists",
          "requires_value": false
        },
        {
          "value": "missing",
          "requires_value": false
        }
      ],
      "page_quantifier": "any_page"
    }
  ]
};
