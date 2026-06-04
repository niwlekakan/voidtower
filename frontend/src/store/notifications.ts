import { create } from 'zustand'

export type NotifLevel = 'info' | 'success' | 'warning' | 'error'

export interface Notification {
  id: string
  level: NotifLevel
  title: string
  message?: string
  duration?: number
}

interface NotificationStore {
  notifications: Notification[]
  add: (n: Omit<Notification, 'id'>) => void
  remove: (id: string) => void
  clear: () => void
}

let counter = 0

export const useNotificationStore = create<NotificationStore>()((set) => ({
  notifications: [],

  add: (n) => {
    const id = `notif-${++counter}`
    const notification: Notification = { duration: 4000, ...n, id }
    set((s) => ({ notifications: [...s.notifications, notification] }))
    if (notification.duration && notification.duration > 0) {
      setTimeout(() => set((s) => ({ notifications: s.notifications.filter((x) => x.id !== id) })), notification.duration)
    }
  },

  remove: (id) => set((s) => ({ notifications: s.notifications.filter((n) => n.id !== id) })),

  clear: () => set({ notifications: [] }),
}))

export const notify = {
  info:    (title: string, message?: string) => useNotificationStore.getState().add({ level: 'info', title, message }),
  success: (title: string, message?: string) => useNotificationStore.getState().add({ level: 'success', title, message }),
  warning: (title: string, message?: string) => useNotificationStore.getState().add({ level: 'warning', title, message }),
  error:   (title: string, message?: string) => useNotificationStore.getState().add({ level: 'error', title, message }),
}
