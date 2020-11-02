# frozen_string_literal: true
require 'pry'

module Exposure
  # Light wrapper around the Core Trace
  class Trace
    def initialize(path_whitelist: //)
      @trace_points = []
      @trace_points << TracePoint.new(:class, :call, &method(:push))
      @trace_points << TracePoint.new(:return, :end, &method(:pop))

      @class_name = Class.instance_method(:to_s)
      @module_name = Module.instance_method(:to_s)
      @is_a = Object.instance_method(:is_a?)

      @path_whitelist = path_whitelist
    end

    def name_of(klass)
      case klass
      when Class then @class_name.bind(klass).call
      when Module then @module_name.bind(klass).call
      end
    rescue
      "(Broken)"
    end

    def start
      @core = Core.new_trace
      @trace_points.each(&:enable)
    end

    def stop
      @trace_points.each(&:disable)
      @core = nil
    end

    def push(trace)
      # puts "#{indent_up} push #{trace.path}"
      analyze_define_method(trace) if trace.method_id == :define_method
      return unless @path_whitelist.match?(trace.path)

      calla = caller_locations(2..2).first
      if @is_a.bind(trace.binding.receiver).call(Class)
        receiver = trace.binding.receiver
      end

      klass = trace.defined_class
      begin
        klass = klass.instance_method(trace.method_id).owner if trace.method_id
      rescue NameError
      end

      # First, push
      Core.push_frame(
        @core,

        trace.event,

        calla.path,
        calla.lineno,

        trace.path,
        trace.lineno,

        name_of(klass),
        trace.method_id.to_s,

        name_of(receiver)
      )

      # Then, add locals
      add_locals(trace.binding)
    end

    def pop(trace)
      # puts "#{indent_down} pop #{trace.path}"
      return unless @path_whitelist.match?(trace.path)

      if trace.event == :return || trace.event == :b_return
        return_class = trace.return_value.class
        return_type = name_of(return_class)
      end

      add_locals(trace.binding)
      Core.pop_frame(@core, return_type)
    end

    private

    def indent
      @indent ||= 0
      ' ' * @indent
    end

    def indent_up
      @indent ||= 0
      @indent += 1
      indent
    end

    def indent_down
      @indent ||= 0
      result = indent
      @indent -= 1
      raise 'IDENT WENT NEGATIVE' if @indent.negative?

      result
    end

    def add_locals(frame_binding)
      frame_binding.local_variables.each do |var|
        begin
          val = frame_binding.local_variable_get(var)
          Core.add_local(@core, var.to_s, name_of(val.class))
        rescue StandardError => e
          Core.add_local(@core, var.to_s, "((#{e.class} during inspect))")
        end
      end
    end

    def analyze_define_method(trace)
      binding.pry
    end
  end
end
