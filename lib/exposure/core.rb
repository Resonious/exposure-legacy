require 'ffi'

module Exposure
  # The FFI interface
  module Core
    extend FFI::Library
    ffi_lib File.join(__dir__, '../../', 'core/target/debug/libexposure.so')

    attach_function :create_trace, [], :pointer
    attach_function :destroy_trace, [:pointer], :void

    enum :event_type, [:b_call, 1, :class, :call, :return, :b_return, :end]
    attach_function :push_frame, [
      :pointer, # trace

      :event_type, # event type

      :string,  # caller file
      :int32,   # caller line

      :string,  # trace file
      :int32,   # trace line

      :string,  # class name
      :string,  # method name

      :string   # receiver name
    ], :void

    attach_function :add_local, [
      :pointer, # trace
      :string,  # var name
      :string   # var type
    ], :void

    attach_function :pop_frame, [
      :pointer, # trace
      :string   # return type
    ], :void

    def self.new_trace
      FFI::AutoPointer.new(create_trace, method(:destroy_trace))
    end
  end
end
