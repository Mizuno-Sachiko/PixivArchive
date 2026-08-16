<script lang="ts">
  import { resolve } from '$app/paths';
</script>

<a class="brand" href={resolve('/overview')} aria-label="PixivArchive概览">
  <img class="mark" src={resolve('/favicon.svg')} alt="" />
  <span class="wordmark">PixivArchive</span>
</a>

<style>
  .brand {
    display: inline-flex;
    align-items: center;
    gap: 0.68rem;
    min-width: max-content;
    font-weight: 760;
    letter-spacing: -0.035em;
  }

  .mark {
    display: block;
    flex: 0 0 auto;
    width: 32px;
    height: 32px;
    border-radius: 9px;
    box-shadow: 0 7px 22px
      color-mix(in srgb, var(--color-primary) 28%, transparent);
  }

  .wordmark {
    font-size: 1.04rem;
  }

  @media (max-width: 520px) {
    .wordmark {
      display: none;
    }
  }
</style>
