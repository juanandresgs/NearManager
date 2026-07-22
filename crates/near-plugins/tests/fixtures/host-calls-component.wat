(component
  (type $resource-ref
    (record
      (field "uri" string)
      (field "provider" string)))
  (type $byte-list (list u8))
  (type $read-result (result $byte-list (error string)))
  (type $message-type
    (func
      (param "kind" string)
      (param "message" string)
      (result (result (error string)))))
  (type $host-type
    (instance
      (export "resource-ref" (type $host-resource-ref (eq $resource-ref)))
      (export "byte-list" (type $host-byte-list (eq $byte-list)))
      (export "read-result" (type $host-read-result (eq $read-result)))
      (export "log" (func (type $message-type)))
      (export "read"
        (func
          (param "target" $host-resource-ref)
          (param "offset" u64)
          (param "length" u32)
          (result $host-read-result)))
      (export "notify" (func (type $message-type)))))
  (import "near:plugin/host@0.1.0" (instance $host (type $host-type)))
  (alias export $host "log" (func $log))
  (alias export $host "read" (func $read))
  (alias export $host "notify" (func $notify))

  (core module $allocator
    (memory (export "memory") 1)
    (global $heap (mut i32) (i32.const 1024))
    (func (export "realloc") (param i32 i32 i32 i32) (result i32)
      (local $address i32)
      global.get $heap
      local.tee $address
      local.get 3
      i32.add
      global.set $heap
      local.get $address))
  (core instance $allocator-instance (instantiate $allocator))
  (alias core export $allocator-instance "memory" (core memory $memory))
  (alias core export $allocator-instance "realloc" (core func $realloc))

  (core func $lower-log
    (canon lower (func $log) (memory $memory) (realloc $realloc)))
  (core func $lower-read
    (canon lower (func $read) (memory $memory) (realloc $realloc)))
  (core func $lower-notify
    (canon lower (func $notify) (memory $memory) (realloc $realloc)))
  (core instance $lowered-host
    (export "log" (func $lower-log))
    (export "read" (func $lower-read))
    (export "notify" (func $lower-notify)))
  (core instance $guest-environment
    (export "memory" (memory $memory)))

  (core module $guest
    (import "host" "log" (func $log (param i32 i32 i32 i32 i32)))
    (import "host" "read" (func $read (param i32 i32 i32 i32 i64 i32 i32)))
    (import "host" "notify" (func $notify (param i32 i32 i32 i32 i32)))
    (import "env" "memory" (memory 1))
    (data (i32.const 0) "info")
    (data (i32.const 16) "canonical host call")
    (data (i32.const 48) "warning")
    (data (i32.const 64) "canonical notification")
    (data (i32.const 96) "fixture://item")
    (data (i32.const 112) "fixture")
    (func (export "probe-log") (result i32)
      i32.const 0
      i32.const 4
      i32.const 16
      i32.const 19
      i32.const 160
      call $log
      i32.const 160
      i32.load)
    (func (export "probe-notify") (result i32)
      i32.const 48
      i32.const 7
      i32.const 64
      i32.const 22
      i32.const 176
      call $notify
      i32.const 176
      i32.load)
    (func (export "probe-read") (result i32)
      i32.const 96
      i32.const 14
      i32.const 112
      i32.const 7
      i64.const 2
      i32.const 3
      i32.const 192
      call $read
      i32.const 192
      i32.load))
  (core instance $guest-instance
    (instantiate $guest
      (with "host" (instance $lowered-host))
      (with "env" (instance $guest-environment))))
  (alias core export $guest-instance "probe-log" (core func $probe-log))
  (alias core export $guest-instance "probe-notify" (core func $probe-notify))
  (alias core export $guest-instance "probe-read" (core func $probe-read))

  (type $probe-type (func (result u32)))
  (func $probe-log-export (type $probe-type) (canon lift (core func $probe-log)))
  (func $probe-notify-export (type $probe-type) (canon lift (core func $probe-notify)))
  (func $probe-read-export (type $probe-type) (canon lift (core func $probe-read)))
  (instance (export "near:plugin/commands@0.1.0")
    (export "probe-log" (func $probe-log-export))
    (export "probe-notify" (func $probe-notify-export))
    (export "probe-read" (func $probe-read-export)))
  (instance (export "near:plugin/provider@0.1.0"))
)
