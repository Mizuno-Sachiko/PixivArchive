declare global {
  namespace App {
    interface PageState {
      detailSource?: import('$lib/stores/detail-navigation').DetailSourceState;
    }
  }
}

export {};
