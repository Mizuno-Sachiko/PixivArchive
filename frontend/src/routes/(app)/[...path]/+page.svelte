<svelte:head>
  <title>页面不存在 · PixivArchive</title>
</svelte:head>

<section class="pending-page">
  <h1>页面不存在</h1>
</section>

<style>
  .pending-page {
    display: grid;
    min-height: min(520px, calc(100vh - 220px));
    place-content: center;
    text-align: center;
  }

  h1 {
    margin: 0;
    font-size: clamp(2rem, 5vw, 3.5rem);
    letter-spacing: -0.05em;
  }
</style>
