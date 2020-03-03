# frozen_string_literal: true

require 'exposure/version'
require 'ffi'

# This is it
module Exposure
  class Error < StandardError; end

  # The FFI interface
  module Core
    extend FFI::Library
    ffi_lib 'core/target/debug/libexposure.so'

    enum :letters, [:b_call, 1, :class, :call, :return, :b_return, :end]
    attach_function :go_and_test, %i[
      int32
      string
      int32
      string
    ], :void
  end

  def self.doit
    Core.go_and_test(3, 'a.rb', 123, 'doit')
    puts 'YEAH!'
  end
end
